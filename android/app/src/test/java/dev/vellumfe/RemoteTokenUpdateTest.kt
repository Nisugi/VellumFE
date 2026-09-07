package dev.vellumfe

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Pairing-token round-trip rules (docs/mobile-runtime-fixes-proposal.md
 * item 9): a token accepted on the remote dashboard updates exactly the
 * saved entry named by its stable store ID via
 * `RemoteStore.withUpdatedToken` — never a name/host guess, never a write
 * when nothing changes.
 */
class RemoteTokenUpdateTest {
    private fun target(
        id: String,
        token: String = "",
        name: String = "Rysk",
        host: String = "10.0.0.5",
        port: Int = 8000,
    ) = RemoteStore.Target(host = host, port = port, token = token, id = id, name = name)

    @Test
    fun updatesExactlyTheEntryWithMatchingId() {
        val targets = listOf(target("id-a"), target("id-b", host = "10.0.0.9"))
        val updated = RemoteStore.withUpdatedToken(targets, "id-b", "tok-9")!!
        assertEquals("", updated[0].token)
        assertEquals("tok-9", updated[1].token)
        // Everything else on the updated entry is untouched.
        assertEquals(targets[1].copy(token = "tok-9"), updated[1])
    }

    @Test
    fun unknownIdIsANoOpEvenWhenNamesMatch() {
        // Same display name on every entry: a stale ID must never fall back
        // to a name match.
        val targets = listOf(target("id-a"), target("id-b", host = "10.0.0.9"))
        assertNull(RemoteStore.withUpdatedToken(targets, "id-gone", "tok"))
    }

    @Test
    fun blankIdOrBlankTokenIsANoOp() {
        val targets = listOf(target("id-a"))
        assertNull(RemoteStore.withUpdatedToken(targets, "", "tok"))
        assertNull(RemoteStore.withUpdatedToken(targets, "id-a", ""))
    }

    @Test
    fun unchangedTokenIsANoOp() {
        // The dashboard re-adopts its token on every refresh; an identical
        // token must not trigger a rewrite of the sealed store.
        val targets = listOf(target("id-a", token = "tok"))
        assertNull(RemoteStore.withUpdatedToken(targets, "id-a", "tok"))
    }

    @Test
    fun replacesARejectedTokenInPlace() {
        val targets = listOf(target("id-a", token = "stale"))
        val updated = RemoteStore.withUpdatedToken(targets, "id-a", "fresh")!!
        assertEquals(listOf(target("id-a", token = "fresh")), updated)
    }
}
