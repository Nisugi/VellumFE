# Mobile → Headless Lich — Integration Plan

Status: planned. Two owners: the **Lich side** (keyword bind hosts +
connect-a-device panel) and the **VellumFE side** (session-control
plumbing + login UI + URL scheme). The only shared dependency is the
deep-link contract below; everything else proceeds in parallel. The
VellumFE side is testable end-to-end against a fake Lich socket without
any Lich-side work landing.

## Goal

The iOS/Android apps are direct-connect only. Add "attach to a headless
Lich session" as a second connection mode, with an onboarding path that
never requires the user to know an IP address, and a repeat path that is
one tap.

Explicit non-goals for v1 (deferred, see bottom): transport auth, the
one-shot `--login` key handoff, a runtime "start listening" Lich command.

## User stories (acceptance targets)

1. **Tailscale user, first time**: installs Tailscale on PC + phone,
   launches `lich --detachable-client=tailscale:8000`, opens Lich's WebUI
   connect panel, points the phone camera at the QR. VellumFE opens with
   host/port prefilled; tap Connect. Never sees an IP.
2. **WireGuard-into-home user, away from home** (no QR available):
   phone's WG tunnel routes the home subnet; Lich binds the LAN IP
   (`lan:8000`). One-time manual entry of `192.168.x.y:8000`, saved as a
   recent. Every later connect — at home or away — is one tap on the
   recents list. The LAN IP is valid on both network paths, so one saved
   entry covers both.
3. **Same-couch user**: browsing Lich's WebUI from the phone browser,
   taps the `vellum://` link on the connect panel (no camera involved);
   VellumFE opens prefilled.
4. **Detach/re-attach**: phone sleeps, loses signal, or force-closes;
   reopening the app re-attaches to the same Lich session with scripts
   still running. (Supervisor semantics for detachable Lich already do
   this — keyless Lich sessions reconnect.)

## The shared contract (agree before building; both sides freeze on it)

Deep link / QR payload — one URI, three renderings (QR image, tappable
`<a>` link, plain text fallback):

```
vellum://lich?host=<host>&port=<port>[&name=<label>]
```

- `host`: IPv4, IPv6 (bracketed), or hostname — the socket layers on
  both ends already resolve names.
- `port`: required, no default in the URI (explicit beats implicit in a
  copy-pasteable artifact).
- `name`: optional display label for the recents list (character or
  machine name). Percent-encoded UTF-8.
- **Nothing secret-shaped goes in this URI** in v1. The transport is
  unauthenticated; isolation comes from the network layer (tailnet /
  WG / LAN). A future Lich session token can ride the same link as an
  extra query param without changing the UX.
- Unknown extra params must be ignored by the receiver (forward compat).

## Lich side (owner: Lich maintainer)

Grounding: `--detachable-client=IP:PORT` and `--bind-address` already
exist (argv_options.rb:179, 185–186); the listener goes through
`ReusableTCPServer.create` → `Addrinfo.tcp`, which resolves hostnames.
The dotted-quad-only restriction is purely the argv regex.

1. **Keyword hosts** in `--detachable-client` (and relax the regex to
   allow hostnames generally):
   - `tailscale:PORT` — scan `Socket.ip_address_list` for a
     `100.64.0.0/10` (CGNAT) address; bind it. If none: fail with
     "Tailscale doesn't appear to be running on this machine."
   - `lan:PORT` — bind the machine's private (RFC1918) address, **with
     the Docker/VM trap handled**: naive first-RFC1918-match will pick a
     Docker bridge (`172.17.0.1`), WSL adapter, or VM host-only
     interface on dev machines. Prefer the interface holding the default
     route; at minimum skip known virtual interface names/ranges before
     falling back. Print the security warning once (unauthenticated
     port, anyone on the LAN can drive the session).
   - `any:PORT` — `0.0.0.0`, louder warning.
2. **Surface the address after bind**: log/echo
   `detachable client listening on <addr>:<port>`.
3. **WebUI "connect a device" panel**: renders the `vellum://` URI as
   (a) a QR code — JS-rendered, self-contained, no new gem/CDN;
   (b) the same URI as a tappable link (covers the phone-browser case);
   (c) the plain `host:port` as selectable text (covers manual entry
   into anything). Fill `name` from the logged-in character when known.

Acceptance: each keyword binds the right interface on a machine that has
Docker + Tailscale + WiFi simultaneously; the panel's three renderings
carry identical host/port; QR scans with a stock iPhone/Android camera
and opens the OS "open in VellumFE?" affordance.

## VellumFE side (owner: Claude, in this repo)

### V1 — protocol + supervisor plumbing

- `ClientMessage::Connect` (frontend/web/protocol.rs) gains
  `mode: direct | lich` (default `direct` — older clients keep working)
  plus `host`/`port` for lich mode. Thread through
  `RemoteEvent::SessionConnect` (core/remote.rs) and
  `SessionRequest::Connect` (frontend/headless/runtime.rs).
- `resolve_connect` returns an enum: `Direct(DirectConnectConfig)` or
  `Lich { host, port, character_label }`. Delete the "web-initiated
  sessions are always direct-mode" assumption.
- `Supervisor` stores the Lich target (today `spawn` reads host/port
  from app config); reconnects re-attach to the web-supplied target.
  `can_reconnect` already treats keyless Lich as re-attachable — keep.
- Profile persistence: `LauncherProfile` already has
  `mode: LaunchMode::Lich` + `host`/`port` fields and a
  "`{char} via Lich @ host:port`" label (config/profiles.rs) — reuse
  as-is. Stop filtering Lich profiles out of the web profiles reply
  (server.rs `profiles_reply`); tag entries with mode so the client can
  render both kinds.
- Tests: fake-Lich TCP fixture (trivial — unauthenticated plaintext
  socket emitting canned Wrayth XML) + integration test alongside
  tests/web_server.rs: web `connect{mode:lich}` → session reaches
  Connected → kill the fake socket → supervisor re-attaches.

### V2 — login screen Lich tab

- Session overlay (app.js/index.html): Direct/Lich mode toggle. Lich
  mode shows host:port (+ optional label) instead of
  account/password/game.
- Recents list renders both profile kinds; tapping a Lich entry
  connects immediately.
- Soft warning when the host is not RFC1918 / CGNAT / loopback /
  `.local` ("this address looks public — the Lich port is
  unauthenticated; use a VPN").
- One-paragraph setup note on the tab (the two happy paths:
  `tailscale:8000` + QR; `lan:8000` + VPN/manual).

### V3 — deep link into both shells

- iOS: `CFBundleURLTypes` for `vellum://`; `onOpenURL` in the SwiftUI
  scene forwards into the web UI. Mechanism already exists: the shells
  boot `/play#token=…` (ContentView.swift:40, MainActivity.kt:94) —
  extend the fragment (e.g. `#token=…&lich=host:port&name=…`) or inject
  via `evaluateJavaScript` when the app is already running.
- Android: `intent-filter` for the scheme; same fragment handoff, plus
  `onNewIntent` for the already-running case.
- Web client: on boot fragment containing a lich target, open the
  session overlay on the Lich tab, prefilled, focus on Connect — never
  auto-connect (the user confirms; guards against malicious QR codes
  pointing the app at attacker sockets).

### V4 — docs

- book/src/frontends/web.md (or a sibling page): the two happy paths,
  the security posture, and the "away from home over WireGuard" recipe
  from user story 2.

Sequencing: V1 → V2 ship together (usable with manual entry — user
story 2 works with zero Lich-side changes). V3 is independent and can
land before or after the Lich panel exists. Estimate: V1+V2 ≈ a day,
V3 ≈ a half day.

## Security posture (v1)

- The detachable port is unauthenticated and can drive the game session;
  Lich can execute arbitrary scripts. Isolation is delegated to the
  network layer: tailnet, WireGuard, or trusted LAN.
- Both sides warn on risky bindings/targets: Lich warns on `lan:`/`any:`
  binds; VellumFE warns on non-private targets.
- No secrets in QR/deep links; no auto-connect from deep links.
- Future (out of scope): Lich-side session token, carried in the same
  deep link and presented on attach.

## Deferred / out of scope for v1

- One-shot `--login` key handoff from the phone (no launcher exists in
  the phone flow to mint keys).
- Runtime `;detach listen` (headless-with-scripts already launches with
  the flag; no "forgot the flag" mid-session story yet).
- `tailscale serve` in docs (second tool + second command; binding the
  tailnet IP gets equivalent isolation with one flag).
- Transport authentication.
