package dev.vellumfe

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.system.Os
import android.util.Log
import java.io.File
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Password-at-rest hardening: a 32-byte master key, wrapped by an
 * AES-256-GCM key that lives in the Android Keystore (hardware-backed,
 * non-exportable), handed to the Rust core via VELLUM_PASSWORD_KEY. The
 * core seals stored password values with it — the passwords file on disk
 * is unreadable without going through this device's Keystore.
 *
 * Fail-closed contract (mirrors src/config/profiles.rs): when no key can
 * be installed, the Rust core DISABLES persistent secret saving instead of
 * writing plaintext. Session-only login keeps working. Failure handling:
 *
 * - TRANSIENT Keystore trouble: leave the stored blob untouched (it may be
 *   fine on the next launch), report [State.UNAVAILABLE].
 * - CONFIRMED-unusable blob (truncated, GCM tag failure, wrong master-key
 *   length): preserve it aside as `pwkey.bin.unusable*` — never deleted —
 *   then reset with a fresh wrapped key. Previously saved passwords CANNOT
 *   be recovered by the new key and must be re-entered; the Rust store
 *   already treats undecryptable `enc:` entries as "re-entry required"
 *   without deleting them.
 */
object CryptoKeys {
    private const val TAG = "VellumShell"
    private const val KEY_ALIAS = "vellum-master"
    private const val KEY_FILE = "pwkey.bin"

    /** Encryption availability, for shell UI/status decisions. */
    enum class State {
        /** installPasswordKey has not run yet. */
        NOT_ATTEMPTED,

        /** Key installed; saved secrets are encrypted at rest. */
        READY,

        /**
         * Key installed after resetting an unusable stored blob (preserved
         * aside). Previously saved passwords require re-entry.
         */
        READY_AFTER_RESET,

        /**
         * No key installed (transient Keystore failure). The Rust core will
         * refuse to persist secrets; session-only login still works.
         */
        UNAVAILABLE,
    }

    @Volatile
    var state: State = State.NOT_ATTEMPTED
        private set

    /** Human-readable detail for the last non-READY outcome ("" when none). */
    @Volatile
    var statusDetail: String = ""
        private set

    val encryptionAvailable: Boolean
        get() = state == State.READY || state == State.READY_AFTER_RESET

    fun installPasswordKey(context: Context) {
        if (encryptionAvailable) return
        synchronized(this) {
            if (encryptionAvailable) return
            try {
                val master = loadOrRecoverMasterKey(context)
                Os.setenv(
                    "VELLUM_PASSWORD_KEY",
                    master.joinToString("") { "%02x".format(it) },
                    true,
                )
                if (state != State.READY_AFTER_RESET) {
                    state = State.READY
                    statusDetail = ""
                }
            } catch (e: Exception) {
                state = State.UNAVAILABLE
                statusDetail = "encryption unavailable: $e"
                Log.w(
                    TAG,
                    "password key unavailable (transient); persistent password " +
                        "saving is disabled for this session, stored key material " +
                        "left untouched: $e",
                )
            }
        }
    }

    private fun keystoreKey(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getEntry(KEY_ALIAS, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
        val generator = KeyGenerator.getInstance(
            KeyProperties.KEY_ALGORITHM_AES,
            "AndroidKeyStore",
        )
        generator.init(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private fun loadOrRecoverMasterKey(context: Context): ByteArray {
        val file = File(context.filesDir, KEY_FILE)
        if (!file.exists()) {
            return createFreshMasterKey(file)
        }
        val blob = file.readBytes()
        if (!KeyBlobPolicy.blobStructurallyValid(blob)) {
            return resetUnusable(file, "key file truncated (${blob.size} bytes)")
        }
        val master = try {
            openBlob(blob)
        } catch (e: Exception) {
            when (KeyBlobPolicy.classifyDecryptFailure(e)) {
                KeyBlobPolicy.Failure.UNUSABLE_BLOB ->
                    return resetUnusable(file, "key file undecryptable: $e")
                // Transient: surface to installPasswordKey, which leaves the
                // stored blob untouched for a later retry.
                KeyBlobPolicy.Failure.TRANSIENT -> throw e
            }
        }
        if (!KeyBlobPolicy.isValidMasterKey(master)) {
            return resetUnusable(file, "decrypted master key has wrong length ${master.size}")
        }
        return master
    }

    /**
     * Explicit reset path for a CONFIRMED-unusable blob: preserve the old
     * bytes aside (never delete), then create a fresh wrapped key. Honest
     * messaging: the old ciphertext is NOT recovered — previously saved
     * passwords must be re-entered.
     */
    private fun resetUnusable(file: File, reason: String): ByteArray {
        val aside = KeyBlobPolicy.preserveAside(file)
            ?: throw IllegalStateException(
                "master key file unusable ($reason) but could not be preserved " +
                    "aside; refusing to overwrite it",
            )
        Log.w(
            TAG,
            "master key file unusable ($reason); preserved as ${aside.name} and " +
                "created a fresh key. Previously saved passwords CANNOT be " +
                "recovered and must be re-entered.",
        )
        val master = createFreshMasterKey(file)
        state = State.READY_AFTER_RESET
        statusDetail = "encryption key was reset ($reason); saved passwords need re-entry"
        return master
    }

    private fun createFreshMasterKey(file: File): ByteArray {
        val master = ByteArray(KeyBlobPolicy.MASTER_KEY_LEN).also { SecureRandom().nextBytes(it) }
        KeyBlobPolicy.writeAtomically(file, sealBlob(master))
        return master
    }

    /** iv ++ AES-256-GCM(plain) under the Keystore wrap key — the same
     * at-rest format as the master key file; RemoteStore reuses it for
     * the Remote tab's saved server. */
    fun sealBlob(plain: ByteArray): ByteArray {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, keystoreKey())
        require(cipher.iv.size == KeyBlobPolicy.GCM_IV_LEN) { "unexpected GCM IV size" }
        return cipher.iv + cipher.doFinal(plain)
    }

    fun openBlob(blob: ByteArray): ByteArray {
        require(blob.size > KeyBlobPolicy.GCM_IV_LEN) { "sealed blob too short" }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(
            Cipher.DECRYPT_MODE,
            keystoreKey(),
            GCMParameterSpec(128, blob.copyOfRange(0, KeyBlobPolicy.GCM_IV_LEN)),
        )
        return cipher.doFinal(blob.copyOfRange(KeyBlobPolicy.GCM_IV_LEN, blob.size))
    }
}
