package dev.vellumfe

import android.content.Context
import android.util.Log
import org.json.JSONObject
import java.io.File

/**
 * The one remembered remote VellumFE server (host, port, pairing token)
 * for the Remote login tab, sealed with the Keystore wrap key from
 * [CryptoKeys] — same trust posture as the master password key: on-device
 * only, unreadable without this device's Keystore. Mirrors the iOS shell's
 * `RemoteStore.swift`.
 */
object RemoteStore {
    private const val TAG = "VellumShell"
    private const val FILE = "remote.bin"

    data class Target(
        val host: String,
        val port: Int,
        /** Pairing token for that PC's web server; empty when the user
         * paired without one (the remote page prompts instead). */
        val token: String,
    )

    fun load(context: Context): Target? {
        val file = File(context.filesDir, FILE)
        if (!file.exists()) return null
        return try {
            val json = JSONObject(String(CryptoKeys.openBlob(file.readBytes()), Charsets.UTF_8))
            Target(
                host = json.getString("host"),
                port = json.getInt("port"),
                token = json.optString("token"),
            )
        } catch (e: Exception) {
            Log.w(TAG, "saved remote server unreadable; forgetting it: $e")
            file.delete()
            null
        }
    }

    fun save(context: Context, target: Target) {
        try {
            val json = JSONObject()
                .put("host", target.host)
                .put("port", target.port)
                .put("token", target.token)
            File(context.filesDir, FILE)
                .writeBytes(CryptoKeys.sealBlob(json.toString().toByteArray(Charsets.UTF_8)))
        } catch (e: Exception) {
            Log.w(TAG, "saving remote server failed: $e")
        }
    }

    fun forget(context: Context) {
        File(context.filesDir, FILE).delete()
    }
}
