# iOS BootModel navigation test plan

Covers item 5 of `mobile-runtime-fixes-proposal.md` (preserve iOS local
navigation during an existing boot). The iOS shell has no XCTest target
(`ios/project.yml` defines only the application target), so these scenarios
are recorded here for manual/simulator verification. If a test target is
added later, `BootModel` needs `CoreBridge.startCore` and `waitForServer`
injected as closures so a delayed boot result can be simulated; the scenarios
below then become unit tests directly.

Design under test: `BootModel.requestedDestination` (`.local` /
`.remote(target)` / `.picker`) is written synchronously by every navigation
entry point before any async work; `localBootInFlight` only tracks core-boot
progress. Boot completion stores port/token unconditionally but renders (or
reports failure) only when `requestedDestination == .local` at completion
time.

## Scenarios

All scenarios assume a delayed boot: `CoreBridge.startCore` (or the health
poll) takes several seconds. On a simulator, delay is natural on first cold
start; artificially extend by breakpointing in `startLocal()` if needed.

1. **local → remote → local ends on local play, one boot.**
   Launch (boot starts, phase `.starting`). Before it completes, navigate to
   a saved remote (picker tap or `vellum://remote?...`) — remote page loads.
   Then request local play again (`vellum://local` from the page, or picker
   "play on this phone"). Expect: phase shows `.starting`, and when the
   original boot completes the local /play URL loads — with no second tap
   and no second `CoreBridge.startCore` call (verify via log/breakpoint:
   `startCore` runs once).

2. **local → remote ends on remote.**
   Launch, then navigate to a remote before boot completes and stay there.
   Expect: remote page remains after boot completion; no flicker to local
   play. Port/token are still recorded (a later "play on this phone" loads
   instantly without another core start).

3. **Latest Lich deep-link prefill survives boot completion.**
   While a boot is in flight, deliver `vellum://lich?host=...&port=...`
   (possibly twice with different hosts). Expect: when the boot completes,
   the loaded /play URL carries the fragment of the LATEST deep link
   (`bootURL` reads `lichFragment` at render time).

4. **Failure after remote navigation does not steal the screen.**
   Force a boot failure (e.g. breakpoint and return an error `CoreInfo`, or
   block the health port so `waitForServer` times out). Before the failure
   lands, navigate to a remote server or the picker. Expect: the remote/
   picker screen stays; no `.failed` page replaces it. Then request local
   play: the boot retries and the failure (if it repeats) now shows.

5. **local → picker ends on picker.**
   Launch, open the picker (`vellum://remote/picker`) before boot completes.
   Expect: picker remains after the boot finishes; core is ready for an
   instant "play on this phone".

6. **Entry-point equivalence.**
   Repeat scenario 1 driving the final local request from each entry point:
   picker "play on this phone" (`startLocal()`), the page's `vellum://local`
   (`showLocal()` now routes through `startLocal()`), and a `vellum://lich`
   deep link. All must behave identically.

Also verify on simulator/device: WebView actually loads each destination URL
and fragment changes trigger a reload (WebViewContainer behavior, outside
BootModel).

## Character-wheel ID resolution (proposal item 7)

iOS has no test target; these scenarios exercise `ContentView.swift`'s
`charsFragment()` / `resolveConnect(_:id:name:)` (which mirrors Android's
JVM-tested `CharacterWheel`). Run on simulator/device with the picker.

1. **Fragment carries parallel IDs.**
   Save two servers. Break at `bootURL` and inspect the fragment: it must
   contain `chars=name@host:port,…&charids=id,…` with IDs in the same order
   as the chars entries, and NO token material for other entries.

2. **Duplicate labels connect to the right server.**
   Save two entries both named "Rysk" at different host/ports. From a remote
   session's touch wheel, pick each one in turn. Expect: each pick loads ITS
   host:port (the wheel sends `vellum://remote/connect?id=…`), never the
   first-saved entry both times.

3. **Independent liveness.**
   With the duplicate pair, keep one desktop reachable and one not. Open the
   touch wheel Characters ring: only the unreachable one dims.

4. **Deleted ID opens the picker.**
   Open a remote session, then delete that duplicate's sibling entry from
   the picker on another visit; fire a wheel pick whose ID no longer exists
   (or call `resolveConnect` with a stale ID in a debugger). Expect: picker,
   never a connect to a different entry — even when the stale request also
   carries a name that still matches one.

5. **Legacy name-only request.**
   Simulate an old desktop-served web client: load
   `vellum://remote/connect?name=Rysk` via the shell handler. With two
   "Rysk" entries: picker (ambiguous). With exactly one match: connects.
   Unknown name: picker.

6. **Rename keeps identity.**
   Rename a saved entry (re-pair same host:port under a new name —
   `RemoteStore.add` preserves the existing `id`). A wheel built after the
   rename still connects to the same server by the same ID.

## Tokenless remote pairing (proposal item 9)

iOS has no test target; the pure classification/fragment rules are covered
by `src/frontend/web/assets/pairing-core.test.mjs` (node --test) and the
Android list edit by `RemoteTokenUpdateTest.kt`. These scenarios exercise
`ContentView.swift`'s side: `showRemote` appending `sid=` and the
`vellum://remote/token` handler updating the Keychain entry by stable ID.
Run on simulator/device against a desktop VellumFE with a pairing token.

1. **Tokenless manual entry leads to pairing, then sessions.**
   Add the desktop server manually WITHOUT a token and connect. Expect: the
   dashboard shows "Pairing required" with a token form — never "No
   sessions running". Enter the desktop's `.webinfo` token. Expect: the
   session list appears without reloading the page.

2. **Accepted token updates the exact saved entry.**
   After scenario 1, return to the picker and connect again. Expect: the
   session list loads immediately (no pairing form) — `updateToken`
   persisted the token on the entry whose `sid` rode the fragment. With two
   saved entries sharing a display name, only the connected one gains the
   token (verify via debugger on `RemoteStore.list()`).

3. **Rejected token can be corrected without deleting the server.**
   Save the server with a WRONG token and connect. Expect: "Access denied"
   plus the form flagged "That pairing token was not accepted" — not an
   empty list. Enter the right token: sessions appear, and the saved entry
   now carries the corrected token.

4. **Return to picker stays available while unpaired.**
   In the pairing state, tap "Back to character picker"
   (`vellum://remote/picker`). Expect: the native picker; the saved server
   is still listed (nothing deleted).

5. **Authenticated empty list still reads as empty.**
   Pair successfully with a desktop that has no running sessions. Expect:
   "No sessions running" (never the pairing form).

6. **Network failure is not "no sessions".**
   Connect to a saved server whose desktop is offline. Expect: "Can't reach
   the server" with a Retry button; Retry after starting the desktop shows
   pairing or sessions as appropriate.

7. **Session navigation retains authentication.**
   From the paired dashboard, open a session card. Expect: the /play URL
   carries the token in the URL FRAGMENT (`#token=…`), never a `?token=`
   query string; the game view authenticates.

8. **Unknown sid is a no-op.**
   Delete the entry on another visit, then replay a stale
   `vellum://remote/token?id=…` (debugger). Expect: no entry changes — the
   handler never falls back to name/host matching.
