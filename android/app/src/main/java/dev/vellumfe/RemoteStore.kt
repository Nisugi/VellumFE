package dev.vellumfe

import android.content.Context
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID

/**
 * The remembered remote VellumFE servers (one per character/port) for the
 * Remote character picker, sealed with the Keystore wrap key from
 * [CryptoKeys] — same trust posture as the master password key: on-device
 * only, unreadable without this device's Keystore. Mirrors the iOS shell's
 * `RemoteStore.swift`.
 *
 * History: this used to hold exactly one `Target`. [list] migrates that
 * legacy single-object blob into a one-element list on first read, so
 * upgrades keep their saved server.
 */
object RemoteStore {
    private const val TAG = "VellumShell"
    private const val FILE = "remote.bin"

    /**
     * One saved remote server. [id] is a stable local handle for delete;
     * [name] is the display label (the character name from a scanned QR, or
     * host:port for a manual entry).
     */
    data class Target(
        val host: String,
        val port: Int,
        /** Pairing token for that PC's web server; empty when the user
         * paired without one (the remote page prompts instead). */
        val token: String,
        val id: String = UUID.randomUUID().toString(),
        val name: String = "$host:$port",
    )

    /** All saved servers, newest last. Migrates a legacy single-item blob. */
    fun list(context: Context): List<Target> {
        val file = File(context.filesDir, FILE)
        if (!file.exists()) return emptyList()
        val text = try {
            String(CryptoKeys.openBlob(file.readBytes()), Charsets.UTF_8)
        } catch (e: Exception) {
            Log.w(TAG, "saved remote servers unreadable; forgetting them: $e")
            file.delete()
            return emptyList()
        }
        return try {
            // Current form: a JSON array of entries.
            parseArray(JSONArray(text))
        } catch (_: Exception) {
            // Legacy single Target (pre-picker): wrap into a one-element list
            // and persist the migrated form so later reads take the fast path.
            try {
                val json = JSONObject(text)
                val migrated = Target(
                    host = json.getString("host"),
                    port = json.getInt("port"),
                    token = json.optString("token"),
                )
                writeBlob(context, listOf(migrated))
                listOf(migrated)
            } catch (e: Exception) {
                Log.w(TAG, "saved remote server unparseable; forgetting it: $e")
                file.delete()
                emptyList()
            }
        }
    }

    /**
     * Add a server, or replace one already saved for the same host:port
     * (re-scanning a character updates its token/name rather than duplicating).
     */
    fun add(context: Context, target: Target) {
        val current = list(context).toMutableList()
        val idx = current.indexOfFirst {
            it.host.equals(target.host, ignoreCase = true) && it.port == target.port
        }
        if (idx >= 0) {
            // Keep the existing id (stable handle), take the new name/token.
            current[idx] = target.copy(id = current[idx].id)
        } else {
            current.add(target)
        }
        writeBlob(context, current)
    }

    /** Remove one saved server by id. Returns the remaining list. */
    fun remove(context: Context, id: String): List<Target> {
        val remaining = list(context).filterNot { it.id == id }
        writeBlob(context, remaining)
        return remaining
    }

    private fun parseArray(array: JSONArray): List<Target> {
        val out = ArrayList<Target>(array.length())
        for (i in 0 until array.length()) {
            val o = array.getJSONObject(i)
            val host = o.getString("host")
            val port = o.getInt("port")
            out.add(
                Target(
                    host = host,
                    port = port,
                    token = o.optString("token"),
                    id = o.optString("id").ifEmpty { UUID.randomUUID().toString() },
                    name = o.optString("name").ifEmpty { "$host:$port" },
                )
            )
        }
        return out
    }

    private fun writeBlob(context: Context, targets: List<Target>) {
        val file = File(context.filesDir, FILE)
        // An empty list clears the file entirely (no stray blob left behind).
        if (targets.isEmpty()) {
            file.delete()
            return
        }
        try {
            val array = JSONArray()
            for (t in targets) {
                array.put(
                    JSONObject()
                        .put("id", t.id)
                        .put("name", t.name)
                        .put("host", t.host)
                        .put("port", t.port)
                        .put("token", t.token)
                )
            }
            file.writeBytes(CryptoKeys.sealBlob(array.toString().toByteArray(Charsets.UTF_8)))
        } catch (e: Exception) {
            Log.w(TAG, "saving remote servers failed: $e")
        }
    }
}
