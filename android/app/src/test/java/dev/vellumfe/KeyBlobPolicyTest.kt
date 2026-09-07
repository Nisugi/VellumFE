package dev.vellumfe

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.security.KeyStoreException
import java.security.ProviderException
import javax.crypto.AEADBadTagException

/**
 * Regression scenarios from docs/mobile-runtime-fixes-proposal.md item 8:
 * classification and atomic-write policy for the wrapped master-key file.
 *
 * Keystore-dependent scenarios that CANNOT run on the plain JVM (the
 * AndroidKeyStore provider does not exist here) — device/emulator test plan:
 *  1. Fresh install: pwkey.bin created, VELLUM_PASSWORD_KEY installed,
 *     CryptoKeys.state == READY; saved password survives an app restart.
 *  2. Truncate pwkey.bin to < 28 bytes, relaunch: file preserved as
 *     pwkey.bin.unusable, fresh key created, state == READY_AFTER_RESET,
 *     old saved passwords require re-entry, new saves encrypted+readable
 *     after restart.
 *  3. Corrupt the last byte of pwkey.bin (GCM tag failure): same as (2).
 *  4. Transient Keystore failure (e.g. locked-device/provider error):
 *     state == UNAVAILABLE, pwkey.bin byte-identical afterward, and the
 *     Rust core refuses password saves (no passwords.toml plaintext).
 *  5. RemoteStore round-trip still works after a master-key reset (it uses
 *     the Keystore wrap key directly, not the master key).
 */
class KeyBlobPolicyTest {

    @get:Rule
    val tmp = TemporaryFolder()

    // --- structural validation -------------------------------------------

    @Test
    fun truncatedBlobsAreStructurallyInvalid() {
        assertFalse(KeyBlobPolicy.blobStructurallyValid(ByteArray(0)))
        assertFalse(KeyBlobPolicy.blobStructurallyValid(ByteArray(12)))
        assertFalse(KeyBlobPolicy.blobStructurallyValid(ByteArray(KeyBlobPolicy.MIN_BLOB_LEN - 1)))
        assertTrue(KeyBlobPolicy.blobStructurallyValid(ByteArray(KeyBlobPolicy.MIN_BLOB_LEN)))
        // IV + tag + 32-byte key: the normal case.
        assertTrue(KeyBlobPolicy.blobStructurallyValid(ByteArray(12 + 16 + 32)))
    }

    @Test
    fun masterKeyLengthIsExactlyThirtyTwoBytes() {
        assertTrue(KeyBlobPolicy.isValidMasterKey(ByteArray(32)))
        assertFalse(KeyBlobPolicy.isValidMasterKey(ByteArray(0)))
        assertFalse(KeyBlobPolicy.isValidMasterKey(ByteArray(16)))
        assertFalse(KeyBlobPolicy.isValidMasterKey(ByteArray(31)))
        assertFalse(KeyBlobPolicy.isValidMasterKey(ByteArray(33)))
        assertFalse(KeyBlobPolicy.isValidMasterKey(ByteArray(64)))
    }

    // --- failure classification ------------------------------------------

    @Test
    fun gcmTagFailureIsConfirmedUnusable() {
        assertEquals(
            KeyBlobPolicy.Failure.UNUSABLE_BLOB,
            KeyBlobPolicy.classifyDecryptFailure(AEADBadTagException("tag mismatch")),
        )
    }

    @Test
    fun keystoreTroubleIsTransientNeverDestructive() {
        assertEquals(
            KeyBlobPolicy.Failure.TRANSIENT,
            KeyBlobPolicy.classifyDecryptFailure(KeyStoreException("keystore not loaded")),
        )
        assertEquals(
            KeyBlobPolicy.Failure.TRANSIENT,
            KeyBlobPolicy.classifyDecryptFailure(ProviderException("provider gone")),
        )
        assertEquals(
            KeyBlobPolicy.Failure.TRANSIENT,
            KeyBlobPolicy.classifyDecryptFailure(RuntimeException("anything unexpected")),
        )
    }

    // --- atomic write -----------------------------------------------------

    @Test
    fun atomicWriteCreatesFileAndLeavesNoTemp() {
        val target = File(tmp.root, "pwkey.bin")
        val bytes = ByteArray(60) { it.toByte() }
        KeyBlobPolicy.writeAtomically(target, bytes)
        assertArrayEquals(bytes, target.readBytes())
        assertFalse(File(tmp.root, "pwkey.bin.tmp").exists())
    }

    @Test
    fun atomicWriteReplacesExistingContentCompletely() {
        val target = File(tmp.root, "pwkey.bin")
        target.writeBytes(ByteArray(100) { 1 })
        val fresh = ByteArray(60) { 2 }
        KeyBlobPolicy.writeAtomically(target, fresh)
        assertArrayEquals(fresh, target.readBytes())
        assertFalse(File(tmp.root, "pwkey.bin.tmp").exists())
    }

    // --- preserve-aside recovery -----------------------------------------

    @Test
    fun preserveAsideKeepsUnusableBytesAndFreesTheName() {
        val target = File(tmp.root, "pwkey.bin")
        val damaged = ByteArray(5) { 9 }
        target.writeBytes(damaged)

        val aside = KeyBlobPolicy.preserveAside(target)

        assertNotNull(aside)
        assertFalse("original name must be free for the fresh key", target.exists())
        assertArrayEquals("preserved bytes must be untouched", damaged, aside!!.readBytes())
        assertEquals("pwkey.bin.unusable", aside.name)
    }

    @Test
    fun preserveAsideNeverOverwritesAnEarlierPreservedBlob() {
        val target = File(tmp.root, "pwkey.bin")
        val first = ByteArray(5) { 1 }
        File(tmp.root, "pwkey.bin.unusable").writeBytes(first)
        val second = ByteArray(5) { 2 }
        target.writeBytes(second)

        val aside = KeyBlobPolicy.preserveAside(target)

        assertNotNull(aside)
        assertEquals("pwkey.bin.unusable.1", aside!!.name)
        assertArrayEquals(second, aside.readBytes())
        assertArrayEquals(
            "earlier preserved blob must survive",
            first,
            File(tmp.root, "pwkey.bin.unusable").readBytes(),
        )
    }
}
