package dev.vellumfe

/**
 * Where the shell should be showing content. Explicit data, not retained
 * callbacks — the latest request always wins over whatever destination was
 * current when a core boot began.
 */
sealed class NavDestination {
    /** Local play against the embedded core (login page front door). */
    object Local : NavDestination()

    /** A saved/paired desktop VellumFE dashboard. */
    data class Remote(val target: RemoteStore.Target) : NavDestination()

    /** The native character picker (no core boot required). */
    object Picker : NavDestination()
}

/**
 * UI-thread-only boot/navigation state machine for MainActivity
 * (docs/mobile-runtime-fixes-proposal.md item 4). Pure Kotlin so the
 * regression scenarios are JVM-testable.
 *
 * Invariants:
 *  - Boot progress and the requested destination are tracked separately.
 *  - Boot requests coalesce into one in-flight operation: while a boot is
 *    in flight, new requests only replace the latest destination.
 *  - Boot completion publishes its port/token once and renders the LATEST
 *    destination, never the one captured when the boot began.
 *  - [invalidate] (activity destroyed) makes every later completion a
 *    no-op so no UI work runs against a dead activity. It never touches
 *    the service-owned core.
 *
 * All methods must be called from the UI thread.
 */
class BootNavState {
    enum class BootPhase { IDLE, IN_FLIGHT, READY, FAILED }

    /** What the caller should do right now. */
    sealed class Effect {
        /** Spawn the (single) core boot thread. */
        object StartBoot : Effect()

        /** Show this destination now. */
        data class Render(val dest: NavDestination) : Effect()

        /** Nothing to do (coalesced into the in-flight boot, or stale). */
        object None : Effect()
    }

    var phase: BootPhase = BootPhase.IDLE
        private set

    /** Published exactly once, by the first successful [onBootSuccess]. */
    var port: Int = -1
        private set
    var token: String? = null
        private set

    /** The latest navigation request; recorded immediately on arrival. */
    var requested: NavDestination? = null
        private set

    private var invalidated = false

    /**
     * Record a navigation request and decide the immediate action. The
     * picker never needs the core; local/remote render at once when the
     * core is ready, start a boot when none is running, and otherwise
     * coalesce into the boot already in flight.
     */
    fun onRequest(dest: NavDestination): Effect {
        if (invalidated) return Effect.None
        requested = dest
        return when {
            dest is NavDestination.Picker -> Effect.Render(dest)
            phase == BootPhase.READY -> Effect.Render(dest)
            phase == BootPhase.IN_FLIGHT -> Effect.None
            else -> { // IDLE or FAILED: (re)start the single boot operation
                phase = BootPhase.IN_FLIGHT
                Effect.StartBoot
            }
        }
    }

    /**
     * The boot thread succeeded. Stores the connection info and returns a
     * render of the latest requested destination — or [Effect.None] when
     * invalidated, stale, or the user has since navigated to the picker.
     */
    fun onBootSuccess(port: Int, token: String): Effect {
        if (invalidated || phase != BootPhase.IN_FLIGHT) return Effect.None
        phase = BootPhase.READY
        this.port = port
        this.token = token
        val dest = requested
        return if (dest == null || dest is NavDestination.Picker) {
            Effect.None
        } else {
            Effect.Render(dest)
        }
    }

    /**
     * The boot thread failed. Returns true when the caller should surface
     * the error (false when invalidated or stale). A later request retries
     * the boot from [BootPhase.FAILED].
     */
    fun onBootFailure(): Boolean {
        if (invalidated || phase != BootPhase.IN_FLIGHT) return false
        phase = BootPhase.FAILED
        return true
    }

    /** Activity destroyed: drop all pending UI work. Core is untouched. */
    fun invalidate() {
        invalidated = true
    }
}
