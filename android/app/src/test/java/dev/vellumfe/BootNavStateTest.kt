package dev.vellumfe

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Regression scenarios from docs/mobile-runtime-fixes-proposal.md item 4:
 * one in-flight boot operation, the latest destination wins regardless of
 * background completion order, and a destroyed activity gets no late UI
 * work.
 */
class BootNavStateTest {

    private fun target(name: String) =
        RemoteStore.Target(host = "10.0.0.$name", port = 8000, token = "t", name = name)

    private fun render(effect: BootNavState.Effect): NavDestination {
        assertTrue("expected Render, got $effect", effect is BootNavState.Effect.Render)
        return (effect as BootNavState.Effect.Render).dest
    }

    @Test
    fun firstBootGatedRequestStartsExactlyOneBoot() {
        val nav = BootNavState()
        assertEquals(BootNavState.Effect.StartBoot, nav.onRequest(NavDestination.Local))
        // Further requests coalesce; no second boot operation.
        assertEquals(BootNavState.Effect.None, nav.onRequest(NavDestination.Remote(target("1"))))
        assertEquals(BootNavState.Effect.None, nav.onRequest(NavDestination.Local))
        assertEquals(BootNavState.BootPhase.IN_FLIGHT, nav.phase)
    }

    @Test
    fun remoteDeepLinkDuringLocalBootWins() {
        val nav = BootNavState()
        nav.onRequest(NavDestination.Local)
        val remote = NavDestination.Remote(target("2"))
        nav.onRequest(remote)
        // Boot finishes after the deep link: it must render remote, not the
        // local destination captured when the boot began.
        assertEquals(remote, render(nav.onBootSuccess(4242, "tok")))
        assertEquals(4242, nav.port)
        assertEquals("tok", nav.token)
    }

    @Test
    fun lastOfSeveralRapidRequestsWins() {
        val nav = BootNavState()
        nav.onRequest(NavDestination.Local)
        nav.onRequest(NavDestination.Remote(target("A")))
        val b = NavDestination.Remote(target("B"))
        nav.onRequest(b)
        assertEquals(b, render(nav.onBootSuccess(1, "x")))
    }

    @Test
    fun remoteThenLocalEndsOnLocal() {
        val nav = BootNavState()
        nav.onRequest(NavDestination.Remote(target("A")))
        nav.onRequest(NavDestination.Local)
        assertEquals(NavDestination.Local, render(nav.onBootSuccess(1, "x")))
    }

    @Test
    fun pickerRendersImmediatelyAndSuppressesBootRender() {
        val nav = BootNavState()
        nav.onRequest(NavDestination.Local)
        // Picker needs no core: renders now.
        assertEquals(NavDestination.Picker, render(nav.onRequest(NavDestination.Picker)))
        // Boot completing later stores the result but must not yank the
        // user off the picker.
        assertEquals(BootNavState.Effect.None, nav.onBootSuccess(1, "x"))
        assertEquals(BootNavState.BootPhase.READY, nav.phase)
        // A later local pick renders immediately with the stored result.
        assertEquals(NavDestination.Local, render(nav.onRequest(NavDestination.Local)))
    }

    @Test
    fun requestsAfterReadyRenderWithoutRebooting() {
        val nav = BootNavState()
        nav.onRequest(NavDestination.Local)
        nav.onBootSuccess(9, "y")
        val remote = NavDestination.Remote(target("C"))
        assertEquals(remote, render(nav.onRequest(remote)))
    }

    @Test
    fun destroyedActivityGetsNoLateUiWork() {
        val nav = BootNavState()
        nav.onRequest(NavDestination.Local)
        nav.invalidate()
        assertEquals(BootNavState.Effect.None, nav.onBootSuccess(1, "x"))
        assertFalse(nav.onBootFailure())
        assertEquals(BootNavState.Effect.None, nav.onRequest(NavDestination.Local))
    }

    @Test
    fun failureSurfacesOnceAndAllowsRetry() {
        val nav = BootNavState()
        nav.onRequest(NavDestination.Local)
        assertTrue(nav.onBootFailure())
        assertEquals(BootNavState.BootPhase.FAILED, nav.phase)
        // Duplicate/stale completions are ignored.
        assertFalse(nav.onBootFailure())
        assertEquals(BootNavState.Effect.None, nav.onBootSuccess(1, "x"))
        // A new request retries the boot.
        assertEquals(BootNavState.Effect.StartBoot, nav.onRequest(NavDestination.Local))
    }
}
