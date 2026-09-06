package dev.vellumfe

/**
 * Three-way classification of a core `/status` poll result.
 *
 * The important distinction is between *confirmed* answers from the core
 * (active or inactive) and *unavailability* (poll failed, or the core
 * reported a state string we don't recognize). Unavailability is not
 * evidence about the session either way — a transient HTTP error while
 * the phone dozes must never be read as "the session ended".
 */
enum class SessionStatus {
    /** Core confirmed a live session (authenticating/connecting/connected/reconnecting). */
    ACTIVE,

    /** Core confirmed no session (idle or disconnected). */
    INACTIVE,

    /** Poll failed or the state string was unrecognized: no evidence either way. */
    UNAVAILABLE;

    companion object {
        private val ACTIVE_STATES =
            setOf("authenticating", "connecting", "connected", "reconnecting")
        private val INACTIVE_STATES = setOf("idle", "disconnected")

        /** Classify a raw state string; null means the poll itself failed. */
        fun classify(state: String?): SessionStatus = when (state) {
            in ACTIVE_STATES -> ACTIVE
            in INACTIVE_STATES -> INACTIVE
            else -> UNAVAILABLE
        }
    }
}

/**
 * Tracks the last *confirmed* activity classification across polls.
 *
 * Rules (see docs/mobile-runtime-fixes-proposal.md, item 3):
 *  - A confirmed poll (ACTIVE or INACTIVE) becomes the new known state.
 *  - An UNAVAILABLE poll preserves the last known classification; the
 *    caller keeps its current wake-lock state and simply retries.
 *  - Before the first confirmed poll, UNAVAILABLE yields UNAVAILABLE:
 *    it must not count as evidence the session ended.
 *
 * Deliberately no failure-count cutoff: a permanently unreachable core
 * stays stoppable through the explicit notification Stop action.
 */
class SessionActivityTracker {
    /** Last confirmed classification; null until the first successful poll. */
    var lastConfirmed: SessionStatus? = null
        private set

    /**
     * Apply one poll result and return the effective classification to act
     * on: the new confirmed state, or on UNAVAILABLE the previous confirmed
     * state (UNAVAILABLE only before any poll has ever succeeded).
     */
    fun onPoll(status: SessionStatus): SessionStatus {
        if (status != SessionStatus.UNAVAILABLE) {
            lastConfirmed = status
            return status
        }
        return lastConfirmed ?: SessionStatus.UNAVAILABLE
    }
}
