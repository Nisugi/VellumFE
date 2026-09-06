package dev.vellumfe

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Regression scenarios from docs/mobile-runtime-fixes-proposal.md item 3:
 * unavailable status must never count as evidence a session ended.
 */
class SessionStatusTest {

    @Test
    fun classifiesConfirmedActiveStates() {
        for (s in listOf("authenticating", "connecting", "connected", "reconnecting")) {
            assertEquals(SessionStatus.ACTIVE, SessionStatus.classify(s))
        }
    }

    @Test
    fun classifiesConfirmedInactiveStates() {
        assertEquals(SessionStatus.INACTIVE, SessionStatus.classify("idle"))
        assertEquals(SessionStatus.INACTIVE, SessionStatus.classify("disconnected"))
    }

    @Test
    fun failedPollAndUnrecognizedStateAreUnavailable() {
        assertEquals(SessionStatus.UNAVAILABLE, SessionStatus.classify(null))
        assertEquals(SessionStatus.UNAVAILABLE, SessionStatus.classify(""))
        assertEquals(SessionStatus.UNAVAILABLE, SessionStatus.classify("some-future-state"))
    }

    @Test
    fun activeThenUnavailableThenActiveStaysActive() {
        val t = SessionActivityTracker()
        assertEquals(SessionStatus.ACTIVE, t.onPoll(SessionStatus.ACTIVE))
        // Transient poll failure: last confirmed classification is preserved.
        assertEquals(SessionStatus.ACTIVE, t.onPoll(SessionStatus.UNAVAILABLE))
        assertEquals(SessionStatus.ACTIVE, t.onPoll(SessionStatus.UNAVAILABLE))
        assertEquals(SessionStatus.ACTIVE, t.onPoll(SessionStatus.ACTIVE))
    }

    @Test
    fun activeThenUnavailableThenIdleBecomesInactive() {
        val t = SessionActivityTracker()
        assertEquals(SessionStatus.ACTIVE, t.onPoll(SessionStatus.ACTIVE))
        assertEquals(SessionStatus.ACTIVE, t.onPoll(SessionStatus.UNAVAILABLE))
        // A later confirmed inactive response is acted on.
        assertEquals(SessionStatus.INACTIVE, t.onPoll(SessionStatus.INACTIVE))
        assertEquals(SessionStatus.INACTIVE, t.lastConfirmed)
    }

    @Test
    fun unavailableBeforeFirstConfirmedPollIsNotEvidenceOfEnd() {
        val t = SessionActivityTracker()
        assertEquals(SessionStatus.UNAVAILABLE, t.onPoll(SessionStatus.UNAVAILABLE))
        assertEquals(SessionStatus.UNAVAILABLE, t.onPoll(SessionStatus.UNAVAILABLE))
        // Never resolved to INACTIVE: the service must not auto-stop.
        assertNull(t.lastConfirmed)
    }

    @Test
    fun confirmedInactiveThenUnavailablePreservesInactive() {
        val t = SessionActivityTracker()
        assertEquals(SessionStatus.INACTIVE, t.onPoll(SessionStatus.INACTIVE))
        assertEquals(SessionStatus.INACTIVE, t.onPoll(SessionStatus.UNAVAILABLE))
    }
}
