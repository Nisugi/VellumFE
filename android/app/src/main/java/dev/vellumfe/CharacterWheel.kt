package dev.vellumfe

/**
 * Character-wheel fragment building and pick resolution
 * (docs/mobile-runtime-fixes-proposal.md item 7). Pure Kotlin — no Android
 * imports — so the regression scenarios are JVM-testable.
 *
 * The shell hands the web client two parallel boot-fragment params:
 * `chars=name@host:port,…` (display) and `charids=id,…` (the saved
 * targets' stable store IDs, same order). A wheel pick round-trips through
 * vellum://remote/connect?id=… and is resolved here BY ID — the name is a
 * display label only, so duplicate labels at different servers stay
 * distinct. Pairing tokens never ride the fragment.
 */
object CharacterWheel {
    /**
     * Both character params for the boot fragment, or null when nothing is
     * saved. Older web clients ignore the unknown `charids=` param and keep
     * their name-based behavior.
     */
    fun fragment(targets: List<RemoteStore.Target>): String? {
        if (targets.isEmpty()) return null
        val chars = targets.joinToString(",") { t ->
            "${percentEncode(t.name)}@${percentEncode(t.host)}:${t.port}"
        }
        val ids = targets.joinToString(",") { percentEncode(it.id) }
        return "chars=$chars&charids=$ids"
    }

    /**
     * Resolve a vellum://remote/connect pick. An explicit non-blank [id]
     * matches only by ID — never falling back to a name — so a stale
     * selection can't be redirected to a different server after a delete.
     * A legacy name-only request resolves only when exactly ONE saved entry
     * carries that name; anything else (ambiguous, unknown, both params
     * absent) returns null and the caller shows the picker.
     */
    fun resolve(
        targets: List<RemoteStore.Target>,
        id: String?,
        name: String?,
    ): RemoteStore.Target? {
        val wantId = id?.trim().orEmpty()
        if (wantId.isNotEmpty()) {
            return targets.find { it.id == wantId }
        }
        val wantName = name?.trim().orEmpty()
        if (wantName.isNotEmpty()) {
            val matches = targets.filter { it.name == wantName }
            return matches.singleOrNull()
        }
        return null
    }

    /**
     * RFC 3986 percent-encoding keeping only unreserved characters, so the
     * web client's decodeURIComponent round-trips any name/host/id exactly.
     */
    fun percentEncode(s: String): String {
        val sb = StringBuilder()
        for (b in s.toByteArray(Charsets.UTF_8)) {
            val c = b.toInt().toChar()
            if (c in 'A'..'Z' || c in 'a'..'z' || c in '0'..'9' ||
                c == '-' || c == '.' || c == '_' || c == '~'
            ) {
                sb.append(c)
            } else {
                sb.append('%')
                sb.append("0123456789ABCDEF"[(b.toInt() shr 4) and 0xF])
                sb.append("0123456789ABCDEF"[b.toInt() and 0xF])
            }
        }
        return sb.toString()
    }
}
