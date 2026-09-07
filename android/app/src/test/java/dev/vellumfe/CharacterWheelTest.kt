package dev.vellumfe

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Character-wheel identity rules (docs/mobile-runtime-fixes-proposal.md
 * item 7): fragment encoding, ID-first resolution, duplicate labels at
 * different endpoints, deletion, rename, and legacy name-only requests.
 */
class CharacterWheelTest {
    private fun target(
        id: String,
        name: String,
        host: String = "10.0.0.5",
        port: Int = 8000,
    ) = RemoteStore.Target(host = host, port = port, token = "tok", name = name, id = id)

    // ---- fragment encoding -------------------------------------------------

    @Test
    fun fragmentIsNullWhenEmpty() {
        assertNull(CharacterWheel.fragment(emptyList()))
    }

    @Test
    fun fragmentCarriesParallelIdsAndEncodes() {
        val frag = CharacterWheel.fragment(
            listOf(
                target("id-a", "Rysk", host = "10.0.0.5"),
                target("id b", "A B", host = "fe80::1", port = 9000),
            ),
        )
        assertEquals(
            "chars=Rysk@10.0.0.5:8000,A%20B@fe80%3A%3A1:9000&charids=id-a,id%20b",
            frag,
        )
    }

    @Test
    fun percentEncodeKeepsUnreservedOnly() {
        assertEquals("a-b._~Z9", CharacterWheel.percentEncode("a-b._~Z9"))
        assertEquals("%40%2C%26%3D", CharacterWheel.percentEncode("@,&="))
        // UTF-8 multibyte survives round-trippably.
        assertEquals("%C3%A9", CharacterWheel.percentEncode("é"))
    }

    // ---- resolution --------------------------------------------------------

    private val dupes = listOf(
        target("id-a", "Rysk", host = "10.0.0.5"),
        target("id-b", "Rysk", host = "10.0.0.9"),
        target("id-c", "Niffy", host = "10.0.0.7"),
    )

    @Test
    fun duplicateLabelsResolveByIdToTheRightServer() {
        assertEquals("10.0.0.5", CharacterWheel.resolve(dupes, "id-a", null)?.host)
        assertEquals("10.0.0.9", CharacterWheel.resolve(dupes, "id-b", null)?.host)
    }

    @Test
    fun unknownOrDeletedIdNeverFallsBackToName() {
        // Name matches an entry, but the explicit ID is gone: picker, not
        // a different server.
        assertNull(CharacterWheel.resolve(dupes, "id-deleted", "Rysk"))
    }

    @Test
    fun legacyNameResolvesOnlyWhenUnique() {
        assertEquals("id-c", CharacterWheel.resolve(dupes, null, "Niffy")?.id)
        assertNull(CharacterWheel.resolve(dupes, null, "Rysk")) // ambiguous
        assertNull(CharacterWheel.resolve(dupes, "", "Rysk")) // blank id = legacy
        assertNull(CharacterWheel.resolve(dupes, null, "Unknown"))
        assertNull(CharacterWheel.resolve(dupes, null, null))
        assertNull(CharacterWheel.resolve(dupes, " ", " "))
    }

    @Test
    fun renameKeepsIdentity() {
        // The store preserves IDs across upserts (RemoteStore.add copies the
        // existing id); a renamed entry still resolves by its old ID.
        val renamed = dupes.map { if (it.id == "id-a") it.copy(name = "Rysk2") else it }
        assertEquals("10.0.0.5", CharacterWheel.resolve(renamed, "id-a", null)?.host)
        // And the OTHER duplicate is now uniquely name-addressable (legacy).
        assertEquals("id-b", CharacterWheel.resolve(renamed, null, "Rysk")?.id)
    }
}
