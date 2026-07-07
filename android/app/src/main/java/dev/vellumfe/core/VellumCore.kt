package dev.vellumfe.core

/**
 * JNI binding to the Rust core (libvellum_android.so — see android/rust).
 *
 * The Rust side is idempotent and thread-safe: both the foreground service
 * and the activity may call [startCore]; whoever calls first boots the
 * runtime, everyone gets the same `{"port": N, "token": "..."}` back.
 */
object VellumCore {
    init {
        System.loadLibrary("vellum_android")
    }

    /** Returns JSON: `{"port": N, "token": "..."}` or `{"error": "..."}`. */
    external fun startCore(dataDir: String): String

    /** Graceful shutdown; blocks until the core thread exits. */
    external fun stopCore()

    /** Returns JSON: `{"running": bool, "port": N?}`. */
    external fun coreStatus(): String
}
