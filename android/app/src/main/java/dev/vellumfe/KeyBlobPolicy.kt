package dev.vellumfe

import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import javax.crypto.AEADBadTagException

/**
 * Pure-JVM policy for the Keystore-wrapped master-key file (`pwkey.bin`):
 * structural validation, failure classification, atomic writes, and
 * preserve-aside recovery. Deliberately free of Android imports so it is
 * unit-testable on the plain JVM (see KeyBlobPolicyTest).
 *
 * The security contract it supports (see [CryptoKeys]):
 * - A CONFIRMED-unusable stored blob (too short to ever decrypt, GCM tag
 *   failure, or a wrong-length decrypted master key) is preserved aside —
 *   never deleted — before a fresh wrapped key is created.
 * - A TRANSIENT failure (Keystore/provider trouble) must not destroy or
 *   replace stored material: the blob may still be recoverable on a later
 *   launch.
 */
object KeyBlobPolicy {
    /** The master key handed to the Rust core is exactly 32 bytes. */
    const val MASTER_KEY_LEN = 32

    /** Sealed-blob layout: 12-byte GCM IV then ciphertext+16-byte tag. */
    const val GCM_IV_LEN = 12
    const val GCM_TAG_LEN = 16

    /** The smallest well-formed sealed blob: IV + tag (empty plaintext). */
    const val MIN_BLOB_LEN = GCM_IV_LEN + GCM_TAG_LEN

    enum class Failure {
        /** Keystore/provider trouble; the stored blob may still be fine. */
        TRANSIENT,

        /** The stored blob can never decrypt under the current wrap key. */
        UNUSABLE_BLOB,
    }

    /** A blob shorter than IV+tag is structurally truncated: confirmed unusable. */
    fun blobStructurallyValid(blob: ByteArray): Boolean = blob.size >= MIN_BLOB_LEN

    fun isValidMasterKey(key: ByteArray): Boolean = key.size == MASTER_KEY_LEN

    /**
     * Classify a decrypt failure. Only an authentication-tag failure proves
     * the stored bytes are undecryptable by the current wrap key; everything
     * else (Keystore unavailable, key entry unreadable, provider errors) is
     * treated as transient so potentially recoverable material survives.
     */
    fun classifyDecryptFailure(e: Exception): Failure = when (e) {
        is AEADBadTagException -> Failure.UNUSABLE_BLOB
        else -> Failure.TRANSIENT
    }

    /**
     * Write [bytes] to [target] atomically: write+fsync a sibling temp file,
     * then rename it over the target. A crash mid-write leaves either the
     * old file or the new one, never a truncated key file.
     */
    @Throws(IOException::class)
    fun writeAtomically(target: File, bytes: ByteArray) {
        val tmp = File(target.parentFile, target.name + ".tmp")
        FileOutputStream(tmp).use { out ->
            out.write(bytes)
            out.fd.sync()
        }
        if (!tmp.renameTo(target)) {
            // Some filesystems refuse rename-over-existing; retry once after
            // removing the target. The temp file still holds the new bytes.
            if (!(target.delete() && tmp.renameTo(target))) {
                tmp.delete()
                throw IOException("atomic rename to ${target.name} failed")
            }
        }
    }

    /**
     * Move a confirmed-unusable blob aside for later recovery. Returns the
     * preserved file, or null when the rename failed (in which case the
     * caller must NOT overwrite the original). Never deletes the blob.
     */
    fun preserveAside(file: File): File? {
        var aside = File(file.parentFile, file.name + ".unusable")
        var n = 1
        while (aside.exists() && n < 1000) {
            aside = File(file.parentFile, "${file.name}.unusable.$n")
            n += 1
        }
        if (aside.exists()) return null
        return if (file.renameTo(aside)) aside else null
    }
}
