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
 * Best-effort: if the Keystore is unavailable the core falls back to its
 * previous behavior (app-private plaintext), never losing saved logins.
 */
object CryptoKeys {
    private const val TAG = "VellumShell"
    private const val KEY_ALIAS = "vellum-master"
    private const val KEY_FILE = "pwkey.bin"
    @Volatile private var installed = false

    fun installPasswordKey(context: Context) {
        if (installed) return
        synchronized(this) {
            if (installed) return
            try {
                val master = loadOrCreateMasterKey(context)
                Os.setenv(
                    "VELLUM_PASSWORD_KEY",
                    master.joinToString("") { "%02x".format(it) },
                    true,
                )
                installed = true
            } catch (e: Exception) {
                Log.w(TAG, "password key unavailable; passwords stored unencrypted: $e")
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

    private fun loadOrCreateMasterKey(context: Context): ByteArray {
        val key = keystoreKey()
        val file = File(context.filesDir, KEY_FILE)
        if (file.exists()) {
            val blob = file.readBytes()
            require(blob.size > 12) { "master key file too short" }
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(
                Cipher.DECRYPT_MODE,
                key,
                GCMParameterSpec(128, blob.copyOfRange(0, 12)),
            )
            return cipher.doFinal(blob.copyOfRange(12, blob.size))
        }
        val master = ByteArray(32).also { SecureRandom().nextBytes(it) }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key)
        require(cipher.iv.size == 12) { "unexpected GCM IV size" }
        file.writeBytes(cipher.iv + cipher.doFinal(master))
        return master
    }
}
