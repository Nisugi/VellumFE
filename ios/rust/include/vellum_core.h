/* C ABI for the VellumFE headless core (libvellum_ios.a).
 *
 * Hand-written; the implementations live in ios/rust/src/lib.rs — keep the
 * two in sync. Every char* returned by these functions is owned by Rust:
 * copy it, then call vellum_string_free exactly once. */

#ifndef VELLUM_CORE_H
#define VELLUM_CORE_H

#ifdef __cplusplus
extern "C" {
#endif

/* Start the headless runtime with all storage under data_dir (the app's
 * Application Support subdirectory). Idempotent: a second call returns the
 * running instance's connection info.
 *
 * Set the VELLUM_PASSWORD_KEY environment variable (64 lowercase hex chars
 * = 32 bytes) *before* the first call so saved passwords are sealed.
 *
 * Returns UTF-8 JSON: {"port": N, "token": "..."} on success,
 * {"error": "..."} on failure. The shell then health-polls
 * http://127.0.0.1:<port>/health and loads
 * http://127.0.0.1:<port>/play#token=<token>. */
char *vellum_start_core(const char *data_dir);

/* Graceful shutdown; blocks until the runtime thread exits. Safe to call
 * when not running. */
void vellum_stop_core(void);

/* Returns UTF-8 JSON: {"running": bool, "port": N}. */
char *vellum_core_status(void);

/* Free a string returned by any vellum_* function. NULL is a no-op. The
 * pointer must not be used afterwards. */
void vellum_string_free(char *s);

#ifdef __cplusplus
}
#endif

#endif /* VELLUM_CORE_H */
