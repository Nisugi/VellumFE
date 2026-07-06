# Mobile Web Frontend — Implementation Plan

Status: in progress — Phases 0, 1, and 2 complete.
Phase 0: serde derives + round-trip test, Color CSS-hex serde, `[web]` config +
`--web-port` flag, axum dependency.
Phase 1: remote ring buffer (`data/remote_buffer.rs`) + `RemoteSink` tap in core,
axum sidecar (`frontend/web/`) with JSON protocol, phone client v0 (text pane,
vitals, RT), wired into both TUI and GUI loops.
Phase 2: client input bar feeding the same command path as local input (echo,
dot-commands, shared history via `record_external_command`), echo/system-message
mirroring into the sink, reconnect resume (session id + full/resume/gap snapshot
modes with a missed-output marker), multi-client fan-out. Verified with socket
integration tests (`tests/web_server.rs`) and live browser sessions against a fake
Lich feed.
Phase 3: origin-tagged menu routing in core (`MenuOrigin` on
`PendingMenuRequest`; local popup path unchanged) plus link-dispatch parity via
`AppCore::resolve_link_activation` — `<d>` tags and coord links (exits) execute
their default command directly, only plain nouns raise a menu, mirroring local
clicks exactly. `link_tap` → per-client `menu` protocol message; picks execute
via the ordinary `cmd` path (items carry ready-made commands, so no `menu_pick`
message was needed). Client: tappable nouns/exits, bottom-sheet menu with four
dismissal paths (pick, ✕, backdrop, tap anywhere else), no-response and
stale-response guards; assets served no-cache. Validated against a real game
session.
Phase 4 (first iteration): stream chips with unread badges (per-stream
client-side buffers, curated hidden-stream list), one-handed bottom chrome
(hands + indicator badges, RT/CT, vitals, input bar lowest), PWA install shell
(manifest + SVG icon + network-first app-shell service worker, iOS metas),
visualViewport keyboard handling, wake-lock toggle, repeat-last +
hold-for-history input. Also fixed a latent core bug: indicator ids matched
case-sensitively so game_state.status never updated. Remaining Phase 4
iteration happens against real phone use.
Phase 5 (manual groups + floating buttons): `macros.toml` in core config —
`[[group]]`s of buttons (action buttons fire on tap; menu buttons open the
bottom sheet; `confirm` gates add a two-step sheet) plus `[[floating]]`
overlay buttons (tap fires, hold drags; positions persist per device in
the browser, not in config). Clients get ids/labels only; taps send
`macro { id }` and the server resolves commands (`MacrosConfig::resolve`).
`.reloadmacros` re-reads the file and pushes to connected phones live.
Phone-side editor: action buttons can be created/edited/deleted from the
phone (+ button on the rail → manager/editor sheets); edits persist to a
separate `macros-local.toml` overlay merged at load, so the hand-written
`macros.toml` is never rewritten. Hand-file buttons are read-only remotely.
Phone editor since gained menu-button (options) editing and per-device
long-press arranging of rail buttons and stream chips. Deferred from
Phase 5: `show_when` context awareness and script-pushed macro sets;
Quickbar reconciliation untouched.
Phase 5b: `/` is the dashboard (session cards by character, browser
health-checked, 10s refresh); the client moved to `/play`. Unpinned
instances port-walk (+20); `pinned = true` binds exactly or fails loudly
via a Notice system message. Pid-keyed registry files in
`~/.vellum-fe/web-sessions/`, removed on clean shutdown, pid-liveness
GC'd after crashes. Verified with two live instances sharing a base port.
Phase 6 (security: pairing token, .webinfo QR, throttle) is the remaining
planned phase.
Target: play a VellumFE session from a phone (Android, iOS, Windows tablet) while the
session stays anchored on the PC behind Lich. Both the desktop frontend and the phone
control the same session simultaneously.

## Decision: browser-based, not native apps

Vellum grows an embedded web server (`frontend/web/`) that serves a touch-first
single-page app over HTTP + WebSocket. The phone opens a browser (or installs the page
as a PWA for a home-screen icon and fullscreen). One codebase covers Android, iOS, and
Windows with no app stores, no Apple developer account, and no mobile toolchains.

Native apps are explicitly a *later, optional* layer: if we ever want push
notifications or haptics, a native shell can speak the exact same WebSocket protocol.
Nothing in this plan is thrown away in that case — the protocol is the investment.

The session-anchored-on-PC topology is what makes mobile viable at all: phones kill
background TCP sockets within seconds of screen-lock. The PC holds the game connection;
the phone is a reconnectable viewport.

## What the code recon established

- **Threading model**: `AppCore` is singly-owned by the active frontend's loop
  (TUI: `frontend/tui/runtime.rs` tokio loop; GUI: `EguiApp` inside eframe). Network
  I/O already talks to the loop via tokio mpsc channels (`server_tx`/`command_rx` in
  `network.rs`). The web server must follow the same pattern — **channels, not
  `Arc<Mutex<AppCore>>`**. Remote client input flows in through a channel the main
  loop drains; state deltas flow out through a `tokio::sync::broadcast` channel.
- **No `UiState` split needed.** Earlier design discussion assumed per-client UI state
  had to be carved out of core. Because the phone client is a browser app, its UI state
  (scroll position, open menus, active macro page, half-typed input) lives in the
  browser. Server-side per-client state reduces to: socket handle, auth state, last
  acknowledged sequence number.
- **Text buffers are pre-wrapped and can't be shipped as-is.** `TextContent.lines`
  (`data/widget.rs`) stores lines already wrapped to the TUI window width. The phone
  needs unwrapped styled lines and lets the browser wrap. So the web frontend needs its
  own **remote scrollback ring buffer** capturing styled-but-unwrapped lines at the
  point in the message pipeline (`core/messages.rs`) where lines are finalized —
  *after* highlighting (CoreHighlightEngine output included), *before* wrapping.
- **Serde groundwork partially exists.** `StyledLine`, `TextSegment`, `QuickbarEntry`
  already derive `Serialize`/`Deserialize`. `Vitals`, `StatusInfo`, `LinkData`,
  compass/room data need an audit and derives added.
- **Links/menus are core-owned already.** `LinkData { exist_id, noun, text, coord }`
  is in the data layer; menu building lives in `core/`. Tappable nouns on the phone
  reuse the `_menu` request path; only the *response routing* needs new work (see
  Phase 3).
- **A Quickbar widget type already exists** (`WidgetType::Quickbar`, `QuickbarEntry`).
  The macro system (Phase 5) should be designed aware of it — game-pushed quickbar
  entries and user-defined macros are siblings, not the same feature.

## Architecture

```
                    ┌────────────────────────────── PC ──────────────────────────────┐
Game ⇄ Lich ⇄ network.rs ⇄ (server_tx/command_rx) ⇄ main loop [AppCore] ⇄ TUI/GUI    │
                                                        │            ▲                │
                                          broadcast<Delta>          mpsc<RemoteEvent> │
                                                        ▼            │                │
                                             frontend/web axum server (tokio task)    │
                    └───────────────────────────────────│────────────────────────────┘
                                                  WS + HTTP
                                                        │
                                              phone browser (PWA)
```

- The web server is a **sidecar to whichever frontend is running**, not a third
  `FrontendType` (initially). TUI mode: spawn the axum task on the existing tokio
  runtime. GUI mode: spawn a dedicated tokio runtime thread; `EguiApp::update()`
  drains the remote-event channel and calls `ctx.request_repaint()` when remote
  activity arrives.
- A true `--frontend web` headless mode (PC/Pi runs Vellum with no local UI) is a
  cheap follow-on once the sidecar exists — it needs only a driver loop that pumps
  network + remote events with no local rendering. Listed as Phase 7, optional.

## Protocol (WebSocket, JSON)

`serde_json` is already a dependency. Envelope: `{ "v": 1, "seq": n, "t": "...", "d": {...} }`.
Every server→client message carries a monotonically increasing `seq` for reconnect resume.

Server → client:
| type | payload | notes |
|---|---|---|
| `hello` | protocol version, character name, stream list | first message |
| `snapshot` | vitals, room (name/desc/exits/players/objects), hands, indicators, active effects, RT/CT end timestamps, server time, last N scrollback lines per stream | on connect / resume-gap-too-big |
| `text` | stream id, `StyledLine` (segments with color + optional `LinkData`) | the hot path |
| `vitals` / `room` / `hands` / `indicators` / `effects` | partial updates | coalesced per frame/batch |
| `rt` | roundtime_end, casttime_end, server_time | client computes countdown locally |
| `menu` | request id, menu items | response to `link_tap`, routed to requesting client only |
| `macros` | active macro groups (from config) | on connect + on config reload |

Client → server:
| type | payload | notes |
|---|---|---|
| `auth` | token | first message; socket dropped on failure |
| `resume` | last seen `seq` | server replays from ring buffer or falls back to `snapshot` |
| `cmd` | command text | enters the same path as locally typed commands (echo, history, dot-commands all behave identically) |
| `link_tap` | exist_id, noun, coord?, request id | server issues `_menu #id` upstream |
| `menu_pick` | request id, item index | executes the menu action |
| `macro` | macro id | server resolves to command(s) from config — client never sends raw script text for macros |

Colors serialize as CSS hex (`#rrggbb`); add a serializer on `frontend/common` color
type or convert at the protocol boundary. Timestamps are game-server time; the client
computes RT/CT countdowns from `rt` + its own clock offset (same technique as
`server_time_offset` in core).

## Phases

Each phase ends runnable and verifiable. One commit per coherent step (layout.rs-style
caution applies anywhere core is touched).

### Phase 0 — Groundwork (small)
- Deps: add `axum` (ws + static serving) — tokio/tower ecosystem already in tree.
- Serde audit: add derives to `Vitals`, `StatusInfo`, `LinkData`, compass/room types,
  effects. Data-layer only; no behavior change.
- Color→hex serialization helper.
- Config: `[web]` section (`enabled = false`, `port = 8040`, `bind = "127.0.0.1"`)
  plus `--web-port` CLI override. Off by default.
- Confirm `tests/architecture.rs` still constrains correctly (web code lives under
  `frontend/`, may import `data/`; core must not import `frontend/web`).

**Exit criteria**: `cargo test` green; a `serde_json::to_string(&styled_line)` round-trip
test exists.

### Phase 1 — Read-only viewer (the proof)
- `frontend/web/mod.rs`, `server.rs`: axum task serving `/` (embedded assets via the
  existing `include_dir` pattern) and `/ws`.
- **Remote scrollback buffer**: new data-layer type (e.g. `data/remote_buffer.rs`) —
  per-stream ring of `(seq, Arc<StyledLine>)`, bounded (default ~2,000 lines per
  stream, configurable). Tap point in `core/messages.rs` where finalized styled lines
  exist pre-wrap. This is the only core-touching change in this phase; keep it a
  single-purpose commit.
  - **Gating**: the buffer is `Option<RemoteBuffer>`, allocated only when
    `web.enabled = true`. Disabled (the default) costs literally nothing — no
    allocation, one `if let None` branch per line. Enabled, the cost is one `Arc`
    clone per finalized line (pointer-sized; the same `Arc<StyledLine>` is shared by
    the ring and the broadcast channel, so the line is never deep-copied) plus a few
    MB of bounded memory. No separate config switch needed beyond `enabled`.
- Broadcast plumbing: after each message batch in the TUI loop (and per `update()` in
  GUI), drain dirty state into `Delta` messages on the broadcast channel. Coalesce
  vitals/room updates per batch.
- Web client v0: single hand-written HTML/JS/CSS bundle, **no npm/bundler** — vanilla
  ES modules, embedded at compile time. Renders: scrolling text pane (styled segments),
  vitals bar, RT countdown. Dark theme.

**Exit criteria**: phone on the same LAN shows live game text and vitals while the TUI
plays normally. (Requires `bind = "0.0.0.0"` consciously set.)

### Phase 2 — Input and dual control
- `cmd` messages feed the same command path as local input (command echo to main
  window, history append, dot-command handling — verify each behaves identically).
- Shared command history: remote commands enter the same history so desk up-arrow
  reaches phone-typed commands.
- Multi-client correctness: N browsers + local frontend all live at once; input
  accepted from all; text broadcast to all.
- Reconnect: client auto-reconnects with `resume`; server replays from ring buffer;
  gap too large → fresh `snapshot` with a "— missed output —" marker.

**Exit criteria**: the bio-break scenario works end-to-end: walk away from the desk,
play from the phone, return to the desk, everything consistent.

### Phase 3 — Links and menus
- Include `LinkData` in serialized text segments; tappable nouns in the web client.
- **Menu response routing** (the one genuinely tricky core change): today a `<menu>`
  response from the game populates the local popup in `ui_state`. Add a pending-menu
  correlation: requests tagged with origin (local vs. client id); the response routes
  to a local popup or a `menu` protocol message accordingly. Design this in
  `core/` with the TUI as first consumer of the refactor so it's provably
  behavior-preserving before the web path is added.
- Web UI: bottom-sheet menu on noun tap; exits in the room bar tappable (move commands).

**Exit criteria**: tap a noun on the phone → menu appears on the phone only → picking
an action executes it. Local TUI menus unchanged.

### Phase 4 — Real mobile UI
- Opinionated default phone layout (not a translation of layout.toml): story text
  fills the screen; slim top bar (room name + tappable exits); bottom chrome — vitals
  strip with RT/CT, hands, input bar. Secondary streams (thoughts, speech, combat…) as
  filter chips or swipe tabs backed by the same stream subscription model.
- PWA: manifest + minimal service worker (app-shell only; never cache the WS),
  `visualViewport` handling so the soft keyboard doesn't cover the input bar, optional
  screen wake-lock toggle.
- Input niceties: command history swipe/long-press, repeat-last-command button.

**Exit criteria**: a session is comfortably playable one-handed for casual
hunting/town use; installs to home screen on Android and iOS.

### Phase 5 — Macro system
- **Definitions live in core config**, not the web frontend: `macros.toml` —
  named groups of `{ label, command, color?, confirm? }`; commands may be `;scripts`
  since Lich sits underneath. Manual group switching first.
- Context awareness second: optional `show_when` conditions per group (room id/name
  pattern, indicator state) evaluated in core — automates what players currently do by
  hand with per-town overlay profiles.
- Web UI: collapsible macro rail/drawer that owns its own space and never overlays the
  text pane.
- Reconcile with the existing `Quickbar` widget/`QuickbarEntry` (game-pushed) — same
  rendering surface, different sources; do not merge the data models without mapping
  consumers first.
- Later (design sketch only for now): script-pushed macro sets — a dot-command or
  protocol tag lets a Lich script publish a temporary button group; state-conditional
  buttons (e.g. herb buttons filtered by current wounds, which core already tracks for
  the injury doll).

**Exit criteria**: a user can define grouped macros in TOML and drive scripts from
phone buttons; groups switch manually (context-switching may land separately).

### Phase 5b — Multi-session dashboard (small, anytime after Phase 2)
One Vellum instance = one session = one web port; players commonly run several
characters at once. Discovery, not multiplexing:

Selection is always explicit — the user picks a session on the dashboard; nothing
ever auto-connects to "whatever is running".

- **Two kinds of URL, strictly separated**:
  - `/` (any instance's port) = the **dashboard**: the session list, by character name.
    It never connects to a game; it only presents choices.
  - `/play` (a specific instance's port) = **that instance's session, and only that
    one**. If that instance isn't running, the bookmark fails visibly (connection
    refused / "session not running" page from another instance is *not* substituted).
    No fallback, no redirect to a different session.
- **Port strategy (serving only)**: an unpinned instance tries the base port (default
  8040) and increments if taken — this decides only *where its server listens*, never
  *which session a user gets*. The recommended phone bookmark is the dashboard
  (`http://<pc>:8040/`), where the user picks by name.
- **Pinned ports for direct bookmarks**: because auto-increment assigns ports by
  launch order, `:8041/play` could be a different character tomorrow. A character's
  profile config may pin `[web] port = 8043`; a pinned instance binds that port or
  **fails loudly** (status-bar warning + log, web disabled for the session) — it never
  silently takes a neighboring port. Pinning is what makes a per-character `/play`
  bookmark stable.
- **Session registry**: on web-server start, each instance writes an entry to a shared
  registry (one file per instance in `~/.vellum-fe/web-sessions/` — per-file avoids
  write races): character, game, port, pid, started-at. Entry removed on clean
  shutdown.
- **Dashboard hosting**: every instance serves the same dashboard at its own `/`, so
  the list is reachable via any live session's port; there is no hub role and nothing
  breaks when the base-port instance exits.
- **Stale entries**: dashboard (or client JS) health-checks each listed port
  (`/health`, short timeout) and hides dead ones; crashed instances leave entries that
  are filtered out the same way and garbage-collected on next startup.
- **Auth**: the pairing token lives in the shared base dir, not per-profile, so one
  pairing covers all sessions and switching characters never re-prompts.
- **Known limitation**: sessions launched with a custom `VELLUM_FE_DIR` use a
  different base dir and won't appear in each other's registry. Acceptable; document it.

### Phase 6 — Security posture
- Defaults stay safe: `enabled = false`, bind `127.0.0.1`.
- Pairing token: generated once into config, required as first WS message; drop on
  failure; throttle attempts. Shown as a QR code / URL by a dot-command
  (`.webinfo`) for easy phone onboarding.
- No TLS of our own: docs state plainly that off-LAN play means Tailscale/WireGuard,
  which also gives encryption for free. An open internet-facing port is explicitly
  unsupported.

### Phase 7 (optional, later) — Headless mode & native shells
- `--frontend web`: driver loop with no local UI (PC or Raspberry Pi as session host).
- Native app shells (notifications for whispers/deaths, haptics) speaking the same
  protocol — only if the PWA proves insufficient.

## Open questions (decide during the relevant phase)
- Delta batching cadence: per message-batch vs. small timer (~50 ms) — measure on
  real 5G latency in Phase 1.
- TabbedText windows: fold into stream chips, or expose tabs? (Phase 4)
- Sounds/TTS on the phone: client-side audio triggered by protocol events is possible;
  out of scope until someone wants it.
- DragonRealms: protocol differences ride on the existing parser; nothing
  mobile-specific expected, but untested.

## Effort (rough, sequential)
| Phase | Size |
|---|---|
| 0 Groundwork | 1–2 days |
| 1 Read-only viewer | ~1 week |
| 2 Input/dual control | 2–3 days |
| 3 Links/menus | ~1 week (menu routing refactor dominates) |
| 4 Mobile UI | 2–3 weeks (the long pole; iterative) |
| 5 Macros | ~1 week for TOML + rail; context/script-driven extras open-ended |
| 5b Session dashboard | 1–2 days |
| 6 Security | 1–2 days |

Phases 0–2 (~2 weeks) produce the "glance at your phone during a hunt" capability,
which is most of the daily value; 3–5 make it a client people switch to.
