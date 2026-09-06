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
