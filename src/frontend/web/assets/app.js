// VellumFE web client — Phase 4 mobile UI.
// Protocol: JSON envelopes { v, seq, t, d } over /ws. See
// docs/mobile-web-frontend-plan.md and src/frontend/web/protocol.rs.

const MAX_BUFFER_LINES = 2000; // per stream, mirrors the server ring
const MAX_DOM_LINES = 1000;    // rendered at once in the pane
const HISTORY_KEY = "vellum-web-history";
const HISTORY_MAX = 50;

// Streams shown as chips. Anything not listed gets a chip with its raw id;
// streams in HIDDEN_STREAMS never render (duplicates of main, or data that
// feeds dedicated widgets rather than prose).
const STREAM_LABELS = {
  main: "Story",
  thoughts: "Thoughts",
  familiar: "Familiar",
  death: "Deaths",
  logons: "Arrivals",
};
const HIDDEN_STREAMS = new Set([
  "room", "inv", "spells", "percWindow", "bounty", "society", "assess",
  "speech", "whisper", "talk", "experience",
]);

const pane = document.getElementById("text-pane");
const chipsBar = document.getElementById("chips");
const topbarTitle = document.getElementById("topbar-title");
const connEl = document.getElementById("conn");
const rtFill = document.getElementById("rt-fill");
const rtLabel = document.getElementById("rt-label");
const handsEl = document.getElementById("hands");
const handLeftEl = document.getElementById("hand-left");
const handRightEl = document.getElementById("hand-right");
const indicatorsEl = document.getElementById("indicators");

// The top bar shows either the room name or the character name; tapping
// it toggles (choice persists per device). Exits are not shown — the
// direction links in the story text are tappable already.
const TITLE_MODE_KEY = "vellum-topbar-mode";
let titleMode = localStorage.getItem(TITLE_MODE_KEY) || "room";
let characterName = null;
let roomName = null;

function renderTitle() {
  const showChar = titleMode === "char";
  const value = showChar ? characterName || roomName : roomName || characterName;
  topbarTitle.textContent = value || "—";
  topbarTitle.classList.toggle(
    "title-char",
    showChar ? !!characterName : !roomName && !!characterName
  );
}

topbarTitle.addEventListener("click", () => {
  titleMode = titleMode === "char" ? "room" : "char";
  try {
    localStorage.setItem(TITLE_MODE_KEY, titleMode);
  } catch { /* fine, just won't persist */ }
  renderTitle();
});

function setCharacter(name) {
  if (!name) return;
  document.title = `${name} — VellumFE`;
  characterName = name;
  renderTitle();
}

const state = {
  lastSeq: 0,          // highest text seq seen (the resume cursor)
  session: null,       // server process id; seqs restart when it changes
  ws: null,
  clockOffset: 0,      // serverTime - localTime, seconds
  rtEnd: null,
  ctEnd: null,
  rtTotal: 0,
  reconnectDelay: 1000,
};

// RT math mirrors the TUI (countdown.rs): the game reports whole seconds
// (prompt time and roundtime end are both truncated), so the offset and
// the remaining count are integer math — fractional local time would
// round nearly everything up a second.
function serverNowInt() {
  return Math.floor(Date.now() / 1000) + state.clockOffset;
}

// Fractional variant, used only to drain the bar smoothly.
function serverNowFrac() {
  return Date.now() / 1000 + state.clockOffset;
}

function setConnected(up) {
  connEl.textContent = up ? "live" : "reconnecting…";
  connEl.className = "conn " + (up ? "conn-up" : "conn-down");
}

// ---- Pairing token ---------------------------------------------------------
// The token arrives via the .webinfo URL fragment (#token=...) on first
// visit and persists in localStorage. Sent as the first WS message; a
// `denied` reply opens the pairing prompt instead of retry-looping.

const TOKEN_KEY = "vellum-token";

function loadToken() {
  const match = location.hash.match(/token=([0-9a-f]+)/i);
  if (match) {
    try {
      localStorage.setItem(TOKEN_KEY, match[1]);
    } catch { /* private mode — works for this visit only */ }
    // Don't leave the token sitting in the address bar / history.
    history.replaceState(null, "", location.pathname);
    return match[1];
  }
  return localStorage.getItem(TOKEN_KEY) || "";
}

let pairingToken = loadToken();
let authDenied = false;

const pairOverlay = document.getElementById("pair-overlay");
document.getElementById("pair-form").addEventListener("submit", (ev) => {
  ev.preventDefault();
  const token = document.getElementById("pair-token").value.trim();
  if (!token) return;
  pairingToken = token;
  try {
    localStorage.setItem(TOKEN_KEY, token);
  } catch { /* private mode */ }
  authDenied = false;
  pairOverlay.hidden = true;
  connect();
});

// ---- Per-stream buffers and chips ---------------------------------------

const buffers = new Map(); // stream -> { lines: [], unread: 0, chip, badge }
let activeStream = "main";

// Chip order is user-arrangeable (long-press a chip) and persists per
// device; streams the user hasn't placed keep their first-text arrival
// order after the placed ones.
const CHIP_ORDER_KEY = "vellum-chip-order";
let chipOrder = [];
try {
  chipOrder = JSON.parse(localStorage.getItem(CHIP_ORDER_KEY) || "[]");
} catch { /* corrupted storage — arrival order it is */ }

function effectiveChipOrder() {
  return [...buffers.keys()].sort((a, b) => {
    const ia = chipOrder.indexOf(a);
    const ib = chipOrder.indexOf(b);
    if (ia !== -1 && ib !== -1) return ia - ib;
    if (ia !== -1) return -1;
    if (ib !== -1) return 1;
    return 0; // stable sort keeps arrival order for unplaced streams
  });
}

function applyChipOrder() {
  for (const stream of effectiveChipOrder()) {
    chipsBar.appendChild(buffers.get(stream).chip);
  }
}

function moveChip(stream, delta) {
  const order = effectiveChipOrder();
  const from = order.indexOf(stream);
  const to = delta === "front" ? 0 : from + delta;
  if (from === -1 || to < 0 || to >= order.length) return;
  order.splice(from, 1);
  order.splice(to, 0, stream);
  chipOrder = order;
  try {
    localStorage.setItem(CHIP_ORDER_KEY, JSON.stringify(chipOrder));
  } catch { /* fine, just won't persist */ }
  applyChipOrder();
}

function openChipArrange(stream) {
  const order = effectiveChipOrder();
  const at = order.indexOf(stream);
  openSheet(`Arrange: ${STREAM_LABELS[stream] || stream}`);
  if (at > 0) sheetButton("⟨  Move left", () => openChipArrangeAfter(stream, -1));
  if (at < order.length - 1) sheetButton("Move right  ⟩", () => openChipArrangeAfter(stream, 1));
  if (at > 0) sheetButton("Move to front", () => openChipArrangeAfter(stream, "front"));
}

// Keep the arrange sheet open across moves so multi-step arranging is
// one gesture; reopening also refreshes which moves are possible.
function openChipArrangeAfter(stream, delta) {
  moveChip(stream, delta);
  openChipArrange(stream);
}

function ensureStream(stream) {
  let buf = buffers.get(stream);
  if (buf) return buf;
  const chip = document.createElement("button");
  chip.type = "button";
  chip.className = "chip";
  const label = document.createElement("span");
  label.textContent = STREAM_LABELS[stream] || stream;
  const badge = document.createElement("span");
  badge.className = "chip-badge";
  badge.hidden = true;
  chip.append(label, badge);
  // Tap switches; long-press opens the arrange sheet.
  let hold = null;
  let held = false;
  chip.addEventListener("pointerdown", () => {
    held = false;
    clearTimeout(hold);
    hold = setTimeout(() => {
      held = true;
      openChipArrange(stream);
    }, 450);
  });
  chip.addEventListener("pointerup", () => clearTimeout(hold));
  chip.addEventListener("pointerleave", () => clearTimeout(hold));
  chip.addEventListener("pointercancel", () => clearTimeout(hold));
  // Long-press must be ours, not the platform context menu / callout.
  chip.addEventListener("contextmenu", (ev) => ev.preventDefault());
  chip.addEventListener("click", () => {
    if (!held) setActiveStream(stream);
  });
  buf = { lines: [], unread: 0, chip, badge };
  buffers.set(stream, buf);
  applyChipOrder();
  updateChips();
  return buf;
}

function updateChips() {
  // Chips bar is pointless with only the story stream.
  chipsBar.hidden = buffers.size <= 1;
  for (const [stream, buf] of buffers) {
    buf.chip.classList.toggle("chip-active", stream === activeStream);
    buf.badge.hidden = buf.unread === 0;
    buf.badge.textContent = buf.unread > 99 ? "99+" : String(buf.unread);
  }
}

function setActiveStream(stream) {
  activeStream = stream;
  const buf = ensureStream(stream);
  buf.unread = 0;
  pendingLines.length = 0;
  const frag = document.createDocumentFragment();
  for (const line of buf.lines.slice(-MAX_DOM_LINES)) frag.appendChild(renderLine(line));
  pane.replaceChildren(frag);
  autoScroll = true;
  scrollToBottom();
  updateChips();
}

// ---- Text pane rendering -------------------------------------------------

function atBottom() {
  return pane.scrollTop + pane.clientHeight >= pane.scrollHeight - 40;
}

function scrollToBottom() {
  pane.scrollTop = pane.scrollHeight;
}

// Autoscroll stickiness is an explicit flag updated on scroll events, not
// re-measured per append: measuring mid-flood reads a stale layout and
// unsticks. The user scrolling up disables it; returning to the bottom
// (or a snapshot reset / stream switch) re-enables it.
let autoScroll = true;
pane.addEventListener("scroll", () => {
  autoScroll = atBottom();
}, { passive: true });

function renderLine(line) {
  const div = document.createElement("div");
  div.className = "line";
  for (const seg of line.segments) {
    if (!seg.text) continue;
    const span = document.createElement("span");
    span.textContent = seg.text;
    if (seg.fg) span.style.color = seg.fg;
    if (seg.bg) span.style.backgroundColor = seg.bg;
    if (seg.bold) span.classList.add("b");
    // Tappable link. The server decides what a tap does, mirroring a
    // local click: <d> tags and coord links run their default command
    // directly (e.g. exits move you); plain nouns get a context menu.
    const link = seg.link_data;
    if (link && link.exist_id) {
      span.classList.add("link");
      span.dataset.existId = link.exist_id;
      span.dataset.noun = link.noun || "";
      span.dataset.text = link.text || "";
      if (link.coord) span.dataset.coord = link.coord;
    }
    div.appendChild(span);
  }
  return div;
}

// Incoming lines for the active stream are queued and rendered once per
// animation frame as a single fragment (per-line appends flood the main
// thread under fast output and break autoscroll).
const pendingLines = [];
let renderScheduled = false;

function flushPendingLines() {
  renderScheduled = false;
  if (!pendingLines.length) return;
  const frag = document.createDocumentFragment();
  for (const line of pendingLines) frag.appendChild(renderLine(line));
  pendingLines.length = 0;
  pane.appendChild(frag);
  while (pane.childElementCount > MAX_DOM_LINES) pane.firstChild.remove();
  if (autoScroll) scrollToBottom();
}

function scheduleRender() {
  if (renderScheduled) return;
  renderScheduled = true;
  requestAnimationFrame(flushPendingLines);
}

function appendText(seq, stream, line) {
  if (seq <= state.lastSeq) return; // duplicate (snapshot/delta overlap)
  state.lastSeq = seq;
  if (HIDDEN_STREAMS.has(stream)) return;
  const buf = ensureStream(stream);
  buf.lines.push(line);
  if (buf.lines.length > MAX_BUFFER_LINES) buf.lines.shift();
  if (stream === activeStream) {
    pendingLines.push(line);
    scheduleRender();
  } else {
    buf.unread += 1;
    updateChips();
  }
}

function appendMarker(text) {
  const div = document.createElement("div");
  div.className = "line marker";
  div.textContent = text;
  pane.appendChild(div);
}

// ---- Status chrome -------------------------------------------------------

function setVitals(v) {
  for (const [key, id] of [
    ["health", "v-health"],
    ["mana", "v-mana"],
    ["stamina", "v-stamina"],
    ["spirit", "v-spirit"],
  ]) {
    const el = document.getElementById(id);
    const pct = Math.max(0, Math.min(100, v[key]));
    el.querySelector(".vital-fill").style.transform = `scaleX(${pct / 100})`;
    el.querySelector(".vital-label").textContent =
      `${key === "health" ? "HP" : key === "mana" ? "MP" : key === "stamina" ? "ST" : "SP"} ${pct}%`;
  }
}

function sendCommand(text) {
  if (!text || !state.ws || state.ws.readyState !== WebSocket.OPEN) return;
  state.ws.send(JSON.stringify({ t: "cmd", d: { text } }));
}

function setRoom(room) {
  roomName = room.name || null;
  renderTitle();
}

function setHands(d) {
  handsEl.hidden = !d.left && !d.right;
  handLeftEl.textContent = d.left || "—";
  handRightEl.textContent = d.right || "—";
  handLeftEl.classList.toggle("empty", !d.left);
  handRightEl.classList.toggle("empty", !d.right);
}

const INDICATOR_BADGES = [
  ["dead", "DEAD", "ind-red"],
  ["stunned", "STUN", "ind-yellow"],
  ["bleeding", "BLEED", "ind-red"],
  ["webbed", "WEB", "ind-yellow"],
  ["hidden", "HIDDEN", "ind-dim"],
  ["invisible", "INVIS", "ind-dim"],
  ["kneeling", "KNEEL", "ind-dim"],
  ["sitting", "SIT", "ind-dim"],
  ["prone", "PRONE", "ind-yellow"],
];

function setIndicators(d) {
  indicatorsEl.replaceChildren();
  for (const [key, label, cls] of INDICATOR_BADGES) {
    if (!d[key]) continue;
    const span = document.createElement("span");
    span.className = `ind ${cls}`;
    span.textContent = label;
    indicatorsEl.appendChild(span);
  }
}

// ---- Active effects --------------------------------------------------------
// Two pills in the status row: ✦ = spells+buffs (count + soonest expiry,
// urgency-colored), ⚠ = debuffs+cooldowns (only rendered when non-empty).
// Tap either for the full sheet; wide viewports show a persistent
// sidebar instead (CSS hides the pills there).

const fxBuffsPill = document.getElementById("fx-buffs");
const fxDebuffsPill = document.getElementById("fx-debuffs");
const effectsPanel = document.getElementById("effects-panel");

const CATEGORY_LABELS = {
  ActiveSpells: "Active Spells",
  Buffs: "Buffs",
  Debuffs: "Debuffs",
  Cooldowns: "Cooldowns",
};

// Categories as last received, each effect annotated with an absolute
// local expiry (so remaining time ticks between server refreshes).
let effectCategories = [];

function parseEffectSeconds(time) {
  const parts = String(time).split(":").map(n => parseInt(n, 10));
  if (!parts.length || parts.some(isNaN)) return null;
  return parts.reduce((total, part) => total * 60 + part, 0);
}

function setEffects(categories) {
  const now = Date.now();
  effectCategories = (categories || []).map(cat => ({
    category: cat.category,
    effects: cat.effects.map(e => {
      const seconds = parseEffectSeconds(e.time);
      return { ...e, expiresAt: seconds === null ? null : now + seconds * 1000 };
    }),
  }));
  renderEffects();
}

function fmtRemaining(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`
    : `${m}:${String(sec).padStart(2, "0")}`;
}

function soonestExpiry(categories) {
  let soonest = null;
  for (const cat of categories) {
    for (const e of cat.effects) {
      if (e.expiresAt !== null && (soonest === null || e.expiresAt < soonest)) {
        soonest = e.expiresAt;
      }
    }
  }
  return soonest;
}

function renderPill(pill, categories, icon) {
  const count = categories.reduce((n, c) => n + c.effects.length, 0);
  if (!count) {
    pill.hidden = true;
    return;
  }
  const soonest = soonestExpiry(categories);
  const remaining = soonest === null ? null : soonest - Date.now();
  pill.hidden = false;
  pill.textContent =
    remaining === null ? `${icon}${count}` : `${icon}${count} ${fmtRemaining(remaining)}`;
  pill.classList.toggle("fx-crit", remaining !== null && remaining < 30_000);
  pill.classList.toggle(
    "fx-warn",
    remaining !== null && remaining >= 30_000 && remaining < 120_000
  );
}

function buildEffectRows(target) {
  target.replaceChildren();
  const now = Date.now();
  for (const cat of effectCategories) {
    if (!cat.effects.length) continue;
    const header = document.createElement("div");
    header.className = "sheet-header";
    header.textContent = CATEGORY_LABELS[cat.category] || cat.category;
    target.appendChild(header);
    const sorted = [...cat.effects].sort(
      (a, b) => (a.expiresAt ?? Infinity) - (b.expiresAt ?? Infinity)
    );
    for (const e of sorted) {
      const row = document.createElement("div");
      row.className = "effect-row";
      const line = document.createElement("div");
      line.className = "fx-line";
      const name = document.createElement("span");
      name.textContent = e.text;
      if (e.text_color) name.style.color = e.text_color;
      const time = document.createElement("span");
      time.className = "fx-time";
      time.textContent = e.expiresAt === null ? e.time : fmtRemaining(e.expiresAt - now);
      line.append(name, time);
      const bar = document.createElement("div");
      bar.className = "fx-bar";
      const fill = document.createElement("div");
      fill.className = "fx-fill";
      fill.style.width = `${Math.max(0, Math.min(100, e.value))}%`;
      if (e.bar_color) fill.style.background = e.bar_color;
      bar.appendChild(fill);
      row.append(line, bar);
      target.appendChild(row);
    }
  }
}

let effectsSheetOpen = false;

function renderEffects() {
  const good = effectCategories.filter(
    c => c.category === "ActiveSpells" || c.category === "Buffs"
  );
  const bad = effectCategories.filter(
    c => c.category === "Debuffs" || c.category === "Cooldowns"
  );
  renderPill(fxBuffsPill, good, "✦");
  renderPill(fxDebuffsPill, bad, "⚠");
  buildEffectRows(effectsPanel);
  if (effectsSheetOpen && !sheet.hidden) buildEffectRows(sheetItems);
}

function openEffectsSheet() {
  openSheet("Effects");
  effectsSheetOpen = true;
  buildEffectRows(sheetItems);
}

fxBuffsPill.addEventListener("click", openEffectsSheet);
fxDebuffsPill.addEventListener("click", openEffectsSheet);

// Tick displayed times locally between server refreshes.
setInterval(() => {
  if (effectCategories.length) renderEffects();
}, 1000);

function setRt(rt) {
  // Every rt message recalibrates the clock (the server sends one per
  // prompt): a roundtime that was flushed ahead of its paired prompt
  // self-corrects on the very next prompt instead of overcounting.
  if (typeof rt.server_time === "number" && rt.server_time > 0) {
    state.clockOffset = rt.server_time - Math.floor(Date.now() / 1000);
  }
  const endsChanged =
    (rt.roundtime_end ?? null) !== state.rtEnd || (rt.casttime_end ?? null) !== state.ctEnd;
  state.rtEnd = rt.roundtime_end ?? null;
  state.ctEnd = rt.casttime_end ?? null;
  // Rebase the bar's full-width reference only when a new timer starts;
  // pure clock resyncs shouldn't make the fill jump.
  if (endsChanged) {
    const end = Math.max(state.rtEnd ?? 0, state.ctEnd ?? 0);
    state.rtTotal = Math.max(end - serverNowInt(), 0);
  }
}

function tickRt() {
  const end = Math.max(state.rtEnd ?? 0, state.ctEnd ?? 0);
  // Integer seconds, exactly like the TUI countdown: 1s of roundtime
  // shows "1", never "2".
  const remaining = end - serverNowInt();
  const isCast = (state.ctEnd ?? 0) >= (state.rtEnd ?? 0) && (state.ctEnd ?? 0) > 0;
  // The bar itself is the panel divider and always visible; only the
  // fill and the little numeric chip come and go.
  if (remaining > 0) {
    const smooth = Math.max(0, end - serverNowFrac());
    const frac = state.rtTotal > 0 ? Math.min(1, smooth / state.rtTotal) : 0;
    rtFill.style.width = `${frac * 100}%`;
    rtFill.style.background = isCast ? "var(--ct)" : "var(--rt)";
    rtLabel.textContent = `${isCast ? "CT" : "RT"} ${remaining}`;
  } else {
    rtFill.style.width = "0";
    rtLabel.textContent = "";
  }
}

// ---- Message handling ----------------------------------------------------

function handleSnapshot(d) {
  // mode: "full" = fresh view; "resume" = only lines newer than our
  // cursor (keep the pane); "gap" = lines were evicted before we could
  // resume (keep the pane, mark the hole).
  if (d.mode === "full") {
    state.lastSeq = 0;
    pendingLines.length = 0;
    for (const [stream, buf] of buffers) {
      buf.lines.length = 0;
      buf.unread = 0;
      if (stream !== "main") {
        buf.chip.remove();
        buffers.delete(stream);
      }
    }
    pane.replaceChildren();
    autoScroll = true;
    updateChips();
  } else if (d.mode === "gap") {
    appendMarker("— missed output —");
  }
  for (const item of d.text) appendText(item.seq, item.stream, item.line);
  flushPendingLines();
  setCharacter(d.character);
  setVitals(d.vitals);
  setRoom(d.room);
  setHands(d.hands || {});
  setIndicators(d.indicators || {});
  setEffects(d.effects || []);
  setRt(d.rt);
  // Sidecar servers (TUI/GUI hosting) don't send session info; treat the
  // session as an implicitly-connected one we can't control.
  setSession(d.session || { state: "connected", session_control: false });
  if (autoScroll) scrollToBottom();
}

function handleMessage(msg) {
  switch (msg.t) {
    case "hello":
      setCharacter(msg.d.character);
      // Seqs restart when the server process changes; drop the cursor.
      if (msg.d.session !== state.session) {
        state.session = msg.d.session;
        state.lastSeq = 0;
      }
      // Answer with our resume cursor; the server replies with a
      // full/resume/gap snapshot accordingly.
      state.ws.send(JSON.stringify({ t: "resume", d: { seq: state.lastSeq } }));
      break;
    case "snapshot": handleSnapshot(msg.d); break;
    case "text": appendText(msg.seq, msg.d.stream, msg.d.line); break;
    case "vitals": setVitals(msg.d); break;
    case "room": setRoom(msg.d); break;
    case "hands": setHands(msg.d); break;
    case "indicators": setIndicators(msg.d); break;
    case "effects": setEffects(msg.d); break;
    case "rt": setRt(msg.d); break;
    case "menu": handleMenu(msg.d); break;
    case "macros": macros = msg.d; renderMacros(); break;
    case "session": setSession(msg.d); break;
    case "profiles": renderProfiles(msg.d.list || []); break;
    case "config_file": handleConfigReply(msg.d); break;
    case "sound": playRemoteSound(msg.d); break;
    case "denied":
      // Wrong/missing token: stop reconnecting, ask for a pairing token.
      authDenied = true;
      try {
        localStorage.removeItem(TOKEN_KEY);
      } catch { /* nothing to clear */ }
      pairOverlay.hidden = false;
      document.getElementById("pair-token").focus();
      break;
    default:
      console.debug("unknown message type", msg.t);
  }
}

function connect() {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  const ws = new WebSocket(`${proto}//${location.host}/ws`);
  state.ws = ws;

  ws.onopen = () => {
    // Pairing token first; everything else follows the hello.
    ws.send(JSON.stringify({ t: "auth", d: { token: pairingToken } }));
    setConnected(true);
    state.reconnectDelay = 1000;
  };
  ws.onmessage = (ev) => {
    try {
      handleMessage(JSON.parse(ev.data));
    } catch (e) {
      console.error("bad message", e);
    }
  };
  ws.onclose = () => {
    setConnected(false);
    if (authDenied) return; // pairing prompt is up; reconnect on submit
    setTimeout(connect, state.reconnectDelay);
    state.reconnectDelay = Math.min(state.reconnectDelay * 2, 10000);
  };
  ws.onerror = () => ws.close();
}

// ---- Session screen (headless runtimes only) ------------------------------
// Servers with session_control (the headless/Android runtime) mirror the
// game-session state machine: idle → authenticating → connecting →
// connected, with reconnecting/disconnected on drops. The overlay is the
// login screen; sidecar servers (TUI/GUI hosting) never advertise the
// capability and never see any of this.

let session = { state: "connected", session_control: false };
let profilesRequested = false;

const sessionOverlay = document.getElementById("session-overlay");
const sessionStatus = document.getElementById("session-status");
const sessionError = document.getElementById("session-error");
const sessionProfiles = document.getElementById("session-profiles");
const sessionForm = document.getElementById("session-form");
const logoutBtn = document.getElementById("logout-btn");
const sessionBanner = document.getElementById("session-banner");

function sendJson(t, d) {
  if (!state.ws || state.ws.readyState !== WebSocket.OPEN) return false;
  state.ws.send(JSON.stringify({ t, d: d || {} }));
  return true;
}

function setSession(d) {
  session = d || { state: "connected", session_control: false };
  updateSessionUi();
}

const SESSION_PROGRESS = {
  authenticating: "Authenticating with play.net…",
  connecting: "Connecting to the game…",
};

function updateSessionUi() {
  if (!session.session_control) {
    sessionOverlay.hidden = true;
    sessionBanner.hidden = true;
    logoutBtn.hidden = true;
    return;
  }

  logoutBtn.hidden = session.state !== "connected";

  // Mid-session drops keep the play view (game text stays useful) and show
  // a banner; the login overlay is for sessions that aren't running.
  if (session.state === "reconnecting") {
    sessionBanner.textContent = session.attempt
      ? `Reconnecting… (attempt ${session.attempt})`
      : "Reconnecting…";
    sessionBanner.hidden = false;
    sessionOverlay.hidden = true;
    return;
  }
  sessionBanner.hidden = true;

  if (session.state === "connected") {
    sessionOverlay.hidden = true;
    profilesRequested = false;
    return;
  }

  // idle / disconnected / authenticating / connecting → overlay.
  const inProgress = session.state in SESSION_PROGRESS;
  sessionStatus.textContent = inProgress ? SESSION_PROGRESS[session.state] : "";
  sessionStatus.hidden = !inProgress;
  sessionError.textContent = session.error || "";
  sessionError.hidden = !session.error;
  sessionForm.querySelectorAll("input, select, button").forEach((el) => {
    el.disabled = inProgress;
  });
  sessionProfiles.classList.toggle("busy", inProgress);
  sessionOverlay.hidden = false;
  if (!profilesRequested && sendJson("get_profiles")) {
    profilesRequested = true;
  }
}

function renderProfiles(list) {
  sessionProfiles.replaceChildren();
  for (const p of list) {
    const row = document.createElement("div");
    row.className = "profile-row";

    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "profile-btn";
    const label = document.createElement("span");
    label.className = "profile-name";
    label.textContent = p.character || p.name;
    const detail = document.createElement("span");
    detail.className = "profile-detail";
    detail.textContent = `${p.name} · ${p.account_masked} · ${p.game}`;
    btn.append(label, detail);
    btn.addEventListener("click", () => {
      if (p.has_password) {
        sendJson("connect", { profile: p.name });
      } else {
        // No stored password: reveal a one-off password prompt for it.
        askProfilePassword(row, p);
      }
    });

    const del = document.createElement("button");
    del.type = "button";
    del.className = "profile-delete";
    del.setAttribute("aria-label", `Delete profile ${p.name}`);
    del.textContent = "✕";
    del.addEventListener("click", (ev) => {
      ev.stopPropagation();
      openSheet(`Delete profile '${p.name}'?`);
      sheetButton("Delete", () => sendJson("delete_profile", { name: p.name }));
      sheetNote("The saved password is removed too.", true);
    });

    row.append(btn, del);
    sessionProfiles.appendChild(row);
  }
}

function askProfilePassword(row, profile) {
  if (row.querySelector(".profile-password")) return;
  const form = document.createElement("form");
  form.className = "profile-password";
  const input = document.createElement("input");
  input.type = "password";
  input.placeholder = "password";
  input.autocomplete = "current-password";
  const go = document.createElement("button");
  go.type = "submit";
  go.textContent = "Connect";
  form.append(input, go);
  form.addEventListener("submit", (ev) => {
    ev.preventDefault();
    if (!input.value) return;
    sendJson("connect", { profile: profile.name, password: input.value });
  });
  row.appendChild(form);
  input.focus();
}

sessionForm.addEventListener("submit", (ev) => {
  ev.preventDefault();
  const account = document.getElementById("login-account").value.trim();
  const password = document.getElementById("login-password").value;
  const character = document.getElementById("login-character").value.trim();
  const game = document.getElementById("login-game").value;
  const save = document.getElementById("login-save").checked;
  if (!account || !password || !character) return;
  sendJson("connect", {
    account,
    password,
    character,
    game,
    save_password: save,
    profile_name: save ? character : null,
  });
  document.getElementById("login-password").value = "";
  // The profile list may gain an entry; refresh next time the overlay shows.
  profilesRequested = false;
});

logoutBtn.addEventListener("click", () => {
  openSheet("Disconnect from the game?");
  sheetButton("Disconnect", () => sendJson("disconnect"));
});

// The reconnect banner is also the escape hatch: tap to stop retrying
// and return to the login screen.
sessionBanner.addEventListener("click", () => {
  openSheet("Stop reconnecting?");
  sheetButton("Stop and log out", () => sendJson("disconnect"));
  sheetNote("Or close this to keep trying.", true);
});

// ---- Settings sheet + config editor ---------------------------------------
// Config files live server-side (on Android, in the app's private storage
// where no file manager reaches) — the editor is how highlights and colors
// get onto the device: paste or import a desktop file, Save validates the
// TOML server-side and hot-reloads.

const EDITOR_FILES = [
  { id: "highlights", label: "Highlights (this profile)", filename: "highlights.toml" },
  { id: "highlights-global", label: "Highlights (global)", filename: "highlights.toml" },
  { id: "colors", label: "Colors (this profile)", filename: "colors.toml" },
  { id: "colors-global", label: "Colors (global)", filename: "colors.toml" },
];

const editorOverlay = document.getElementById("editor-overlay");
const editorTitle = document.getElementById("editor-title");
const editorText = document.getElementById("editor-text");
const editorStatus = document.getElementById("editor-status");
let editorFile = null; // active EDITOR_FILES entry
let configRequestCounter = 0;
let pendingConfigRequest = null;

document.getElementById("settings-btn").addEventListener("click", () => {
  openSheet("Settings");
  sheetButton(
    soundMuted ? "Sound alerts: off — tap to enable" : "Sound alerts: on — tap to mute",
    () => {
      soundMuted = !soundMuted;
      try {
        localStorage.setItem(SOUND_MUTE_KEY, soundMuted ? "1" : "");
      } catch { /* private mode */ }
    },
  );
  for (const file of EDITOR_FILES) {
    sheetButton(file.label, () => openConfigEditor(file));
  }
});

// ---- Sound alerts ----------------------------------------------------------
// Highlight-triggered sounds arrive as `sound` messages; the browser is
// the audio device (the Android core has no native audio). Files are
// fetched from /sounds/ with the pairing token.

const SOUND_MUTE_KEY = "vellum-sound-muted";
let soundMuted = false;
try {
  soundMuted = !!localStorage.getItem(SOUND_MUTE_KEY);
} catch { /* default unmuted */ }

function playRemoteSound(d) {
  if (soundMuted || !d.file) return;
  const url = `/sounds/${encodeURIComponent(d.file)}?token=${encodeURIComponent(pairingToken)}`;
  const audio = new Audio(url);
  if (typeof d.volume === "number") {
    audio.volume = Math.max(0, Math.min(1, d.volume));
  }
  audio.play().catch((e) => {
    // Autoplay policy: needs one interaction first; normal before login.
    console.debug("sound blocked or missing", d.file, e);
  });
}

function editorStatusMsg(text, isError) {
  editorStatus.textContent = text;
  editorStatus.classList.toggle("editor-error", !!isError);
  editorStatus.hidden = !text;
}

function openConfigEditor(file) {
  editorFile = file;
  editorTitle.textContent = file.label;
  editorText.value = "";
  editorText.disabled = true;
  editorStatusMsg("Loading…", false);
  editorOverlay.hidden = false;
  pendingConfigRequest = ++configRequestCounter;
  sendJson("config_get", { request_id: pendingConfigRequest, file: file.id });
}

function handleConfigReply(d) {
  if (d.request_id !== pendingConfigRequest) return;
  if (d.error) {
    editorStatusMsg(d.error, true);
    editorText.disabled = false;
    return;
  }
  if (d.saved) {
    editorStatusMsg("Saved — applied live.", false);
    editorText.disabled = false;
    return;
  }
  if (typeof d.content === "string") {
    editorText.value = d.content;
    editorText.disabled = false;
    editorStatusMsg(d.content ? "" : "Empty — paste or import a file.", false);
  }
}

document.getElementById("editor-close").addEventListener("click", () => {
  editorOverlay.hidden = true;
  editorFile = null;
});

document.getElementById("editor-save").addEventListener("click", () => {
  if (!editorFile) return;
  editorStatusMsg("Saving…", false);
  pendingConfigRequest = ++configRequestCounter;
  sendJson("config_put", {
    request_id: pendingConfigRequest,
    file: editorFile.id,
    content: editorText.value,
  });
});

document.getElementById("editor-export").addEventListener("click", () => {
  if (!editorFile) return;
  const blob = new Blob([editorText.value], { type: "text/plain" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = editorFile.filename;
  a.click();
  URL.revokeObjectURL(a.href);
});

const editorFileInput = document.getElementById("editor-file");
document.getElementById("editor-import").addEventListener("click", () => {
  editorFileInput.click();
});
editorFileInput.addEventListener("change", () => {
  const picked = editorFileInput.files && editorFileInput.files[0];
  if (!picked) return;
  picked.text().then((text) => {
    editorText.value = text;
    editorStatusMsg("Imported — review, then Save.", false);
  });
  editorFileInput.value = "";
});

// ---- Bottom sheet (noun menus + local pickers) ---------------------------

const sheet = document.getElementById("sheet");
const sheetBackdrop = document.getElementById("sheet-backdrop");
const sheetTitle = document.getElementById("sheet-title");
const sheetItems = document.getElementById("sheet-items");

let menuRequestCounter = 0;
let pendingMenuRequest = null;
let sheetTimeout = null;

function closeSheet() {
  sheet.hidden = true;
  sheetBackdrop.hidden = true;
  pendingMenuRequest = null;
  effectsSheetOpen = false;
  clearTimeout(sheetTimeout);
}

function openSheet(title) {
  sheetTitle.textContent = title;
  sheetItems.replaceChildren();
  effectsSheetOpen = false;
  sheet.hidden = false;
  sheetBackdrop.hidden = false;
}

function sheetNote(text, dismisses) {
  const div = document.createElement("div");
  div.className = "sheet-empty";
  div.textContent = text;
  if (dismisses) div.addEventListener("click", closeSheet);
  sheetItems.appendChild(div);
  return div;
}

function sheetButton(text, onPick) {
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "sheet-item";
  btn.textContent = text;
  btn.addEventListener("click", () => {
    // Close first: onPick may open a follow-up sheet (confirm steps).
    closeSheet();
    onPick();
  });
  sheetItems.appendChild(btn);
  return btn;
}

function openSheetLoading(noun) {
  openSheet(noun);
  sheetNote("…", false);
  // Never leave the sheet spinning if the response is lost (disconnect,
  // stale request id, server restart).
  clearTimeout(sheetTimeout);
  sheetTimeout = setTimeout(() => {
    if (!sheet.hidden && pendingMenuRequest !== null) {
      sheetItems.replaceChildren();
      sheetNote("No response — tap to dismiss", true);
    }
  }, 5000);
}

sheetBackdrop.addEventListener("click", closeSheet);
document.getElementById("sheet-close").addEventListener("click", closeSheet);

// Belt and braces: while the sheet is open, tapping anything that is not
// the sheet itself (and not a link, which retargets the sheet) closes it.
document.addEventListener("click", (ev) => {
  if (sheet.hidden) return;
  if (ev.target.closest("#sheet")) return;
  // A picked sheet item may already be detached (the pick re-rendered the
  // sheet, e.g. option -> confirm step); closest("#sheet") can't see that,
  // but the class on the detached node itself still matches.
  if (ev.target.closest(".sheet-item, .sheet-empty")) return;
  if (ev.target.closest("span.link")) return;
  if (ev.target.closest("#repeat-btn")) return; // long-press opens history
  if (ev.target.closest("#macro-rail")) return; // rail taps retarget the sheet
  if (ev.target.closest("#chips")) return; // long-press opens arrange
  if (ev.target.closest(".fx-pill")) return; // opens the effects sheet
  if (ev.target.closest("#textsize-btn")) return; // opens the size stepper
  if (ev.target.closest(".float-btn")) return;
  closeSheet();
});

pane.addEventListener("click", (ev) => {
  const span = ev.target.closest("span.link");
  if (!span || !state.ws || state.ws.readyState !== WebSocket.OPEN) return;
  const requestId = ++menuRequestCounter;
  // Direct links (<d> tags, coord links like exits) execute immediately
  // server-side — no menu will come back, so no sheet.
  const isDirect = span.dataset.existId === "_direct_" || span.dataset.coord;
  if (isDirect) {
    closeSheet();
  } else {
    pendingMenuRequest = requestId;
    openSheetLoading(span.dataset.noun || span.textContent);
  }
  state.ws.send(JSON.stringify({
    t: "link_tap",
    d: {
      request_id: requestId,
      exist_id: span.dataset.existId,
      noun: span.dataset.noun || "",
      text: span.dataset.text || span.textContent,
      coord: span.dataset.coord || null,
    },
  }));
});

function handleMenu(d) {
  // Stale or superseded response (user tapped something else meanwhile).
  if (d.request_id !== pendingMenuRequest) return;
  clearTimeout(sheetTimeout);
  sheetTitle.textContent = d.noun || sheetTitle.textContent;
  sheetItems.replaceChildren();
  let rendered = 0;
  for (const item of d.items) {
    if (item.disabled) {
      const header = document.createElement("div");
      header.className = "sheet-header";
      header.textContent = item.text;
      sheetItems.appendChild(header);
      continue;
    }
    // Defense in depth: never execute client-internal sentinels.
    if (!item.command || /^(__|action:|menu:)/.test(item.command)) continue;
    sheetButton(item.text, () => sendCommand(item.command));
    rendered += 1;
  }
  if (rendered === 0) sheetNote("No actions available", true);
}

// ---- Macro buttons ---------------------------------------------------------
// Definitions come from the server (macros.toml); taps send back opaque
// ids that the server resolves to commands. An action button fires on
// tap; a menu button opens the bottom sheet; `confirm` inserts a
// two-step confirm sheet. Floating buttons overlay the text pane — tap
// to fire, hold to drag; positions persist per device.

const macroRail = document.getElementById("macro-rail");
const macroGroupBtn = document.getElementById("macro-group-btn");
const macroButtonsEl = document.getElementById("macro-buttons");
const macroCollapseBtn = document.getElementById("macro-collapse");
const floatingLayer = document.getElementById("floating-layer");

const GROUP_KEY = "vellum-macro-group";
const COLLAPSE_KEY = "vellum-macro-collapsed";
const FLOAT_POS_KEY = "vellum-float-pos";

let macros = null;

function sendMacro(id) {
  if (!state.ws || state.ws.readyState !== WebSocket.OPEN) return;
  state.ws.send(JSON.stringify({ t: "macro", d: { id } }));
}

function confirmSheet(label, onConfirm) {
  openSheet(label);
  sheetButton(`Confirm: ${label}`, onConfirm);
  sheetNote("tap anywhere else to cancel", true);
}

function activateMacro(btn) {
  if (btn.options && btn.options.length) {
    openSheet(btn.label);
    for (const opt of btn.options) {
      sheetButton(opt.label, () => {
        if (opt.confirm) confirmSheet(opt.label, () => sendMacro(opt.id));
        else sendMacro(opt.id);
      });
    }
    return;
  }
  if (btn.confirm) confirmSheet(btn.label, () => sendMacro(btn.id));
  else sendMacro(btn.id);
}

function currentGroup() {
  if (!macros || !macros.groups.length) return null;
  const savedName = localStorage.getItem(GROUP_KEY);
  return macros.groups.find((g) => g.name === savedName) || macros.groups[0];
}

// Per-device button order within each group (long-press a rail button to
// arrange). Works for hand-file buttons too since it never touches the
// server: { [groupName]: [label, ...] }.
const MACRO_ORDER_KEY = "vellum-macro-order";
let macroOrder = {};
try {
  macroOrder = JSON.parse(localStorage.getItem(MACRO_ORDER_KEY) || "{}");
} catch { /* corrupted storage — config order it is */ }

function orderedButtons(group) {
  const placed = macroOrder[group.name] || [];
  return [...group.buttons].sort((a, b) => {
    const ia = placed.indexOf(a.label);
    const ib = placed.indexOf(b.label);
    if (ia !== -1 && ib !== -1) return ia - ib;
    if (ia !== -1) return -1;
    if (ib !== -1) return 1;
    return 0; // stable: config order for unplaced buttons
  });
}

function moveMacroButton(group, label, delta) {
  const order = orderedButtons(group).map((b) => b.label);
  const from = order.indexOf(label);
  const to = delta === "front" ? 0 : from + delta;
  if (from === -1 || to < 0 || to >= order.length) return;
  order.splice(from, 1);
  order.splice(to, 0, label);
  macroOrder[group.name] = order;
  try {
    localStorage.setItem(MACRO_ORDER_KEY, JSON.stringify(macroOrder));
  } catch { /* fine, just won't persist */ }
  renderMacros();
}

function openMacroArrange(group, btn) {
  const order = orderedButtons(group).map((b) => b.label);
  const at = order.indexOf(btn.label);
  openSheet(btn.label);
  if (btn.editable) {
    sheetButton("Edit…", () => openMacroEditor({ group: group.name, btn }));
  }
  const reopen = (delta) => {
    moveMacroButton(group, btn.label, delta);
    openMacroArrange(group, btn);
  };
  if (at > 0) sheetButton("⟨  Move left", () => reopen(-1));
  if (at < order.length - 1) sheetButton("Move right  ⟩", () => reopen(1));
  if (at > 0) sheetButton("Move to front", () => reopen("front"));
}

function renderMacros() {
  renderFloating();
  // Rail shows once definitions arrive, even empty: the + button is how
  // the first macro gets created from the phone.
  macroRail.hidden = macros === null;
  const group = currentGroup();
  macroGroupBtn.hidden = !group;
  macroButtonsEl.replaceChildren();
  if (!group) return;
  macroGroupBtn.textContent =
    macros.groups.length > 1 ? `${group.name} ▾` : group.name;
  for (const btn of orderedButtons(group)) {
    const el = document.createElement("button");
    el.type = "button";
    el.className = "macro-btn";
    el.textContent = btn.options && btn.options.length ? `${btn.label} ›` : btn.label;
    if (btn.color) {
      el.style.background = "none";
      el.style.border = `1px solid ${btn.color}`;
      el.style.color = btn.color;
    }
    // Tap fires; long-press arranges (and edits, for phone-authored).
    let hold = null;
    let held = false;
    el.addEventListener("pointerdown", () => {
      held = false;
      clearTimeout(hold);
      hold = setTimeout(() => {
        held = true;
        openMacroArrange(group, btn);
      }, 450);
    });
    el.addEventListener("pointerup", () => clearTimeout(hold));
    el.addEventListener("pointerleave", () => clearTimeout(hold));
    el.addEventListener("pointercancel", () => clearTimeout(hold));
    el.addEventListener("contextmenu", (ev) => ev.preventDefault());
    el.addEventListener("click", () => {
      if (!held) activateMacro(btn);
    });
    macroButtonsEl.appendChild(el);
  }
  const collapsed = localStorage.getItem(COLLAPSE_KEY) === "1";
  macroButtonsEl.hidden = collapsed;
  macroCollapseBtn.textContent = collapsed ? "▸" : "▾";
}

macroGroupBtn.addEventListener("click", () => {
  if (!macros || macros.groups.length <= 1) return;
  openSheet("Macro group");
  for (const group of macros.groups) {
    sheetButton(group.name, () => {
      localStorage.setItem(GROUP_KEY, group.name);
      renderMacros();
    });
  }
});

macroCollapseBtn.addEventListener("click", () => {
  const collapsed = localStorage.getItem(COLLAPSE_KEY) === "1";
  localStorage.setItem(COLLAPSE_KEY, collapsed ? "0" : "1");
  renderMacros();
});

function floatPositions() {
  try {
    return JSON.parse(localStorage.getItem(FLOAT_POS_KEY) || "{}");
  } catch {
    return {};
  }
}

function renderFloating() {
  floatingLayer.replaceChildren();
  if (!macros) return;
  const saved = floatPositions();
  macros.floating.forEach((btn, i) => {
    const el = document.createElement("button");
    el.type = "button";
    el.className = "float-btn";
    el.textContent = btn.label;
    if (btn.color) {
      el.style.borderColor = btn.color;
      el.style.color = btn.color;
    }
    // Device-local position wins; TOML x/y is the starting point; new
    // buttons stack down the right edge.
    const pos = saved[btn.id] || [btn.x ?? 0.86, btn.y ?? 0.18 + i * 0.12];
    el.style.left = `${pos[0] * 100}%`;
    el.style.top = `${pos[1] * 100}%`;
    attachFloatBehavior(el, btn);
    floatingLayer.appendChild(el);
  });
}

function attachFloatBehavior(el, btn) {
  let holdTimer = null;
  let dragging = false;
  el.addEventListener("pointerdown", (ev) => {
    dragging = false;
    clearTimeout(holdTimer);
    holdTimer = setTimeout(() => {
      dragging = true;
      el.classList.add("dragging");
      el.setPointerCapture(ev.pointerId);
    }, 450);
  });
  el.addEventListener("pointermove", (ev) => {
    if (!dragging) return;
    const rect = floatingLayer.getBoundingClientRect();
    const x = Math.min(0.95, Math.max(0.05, (ev.clientX - rect.left) / rect.width));
    const y = Math.min(0.95, Math.max(0.05, (ev.clientY - rect.top) / rect.height));
    el.style.left = `${x * 100}%`;
    el.style.top = `${y * 100}%`;
    el.dataset.fx = x;
    el.dataset.fy = y;
  });
  const endDrag = () => {
    clearTimeout(holdTimer);
    if (dragging && el.dataset.fx) {
      const saved = floatPositions();
      saved[btn.id] = [parseFloat(el.dataset.fx), parseFloat(el.dataset.fy)];
      try {
        localStorage.setItem(FLOAT_POS_KEY, JSON.stringify(saved));
      } catch { /* storage blocked — position just won't persist */ }
    }
    el.classList.remove("dragging");
    // Cleared on a timeout so the click that follows pointerup still
    // sees dragging=true and skips activation.
    setTimeout(() => {
      dragging = false;
    }, 0);
  };
  el.addEventListener("pointerup", endDrag);
  el.addEventListener("pointercancel", endDrag);
  el.addEventListener("contextmenu", (ev) => ev.preventDefault());
  el.addEventListener("click", () => {
    if (!dragging) activateMacro(btn);
  });
}

// ---- Macro editor ----------------------------------------------------------
// Phone-authored buttons live in macros-local.toml on the PC; the server
// applies edits and re-broadcasts, so every client (and the desk) sees
// the change instantly. Hand-file buttons are read-only here.

const COLOR_PRESETS = [null, "#d9b44f", "#d9534f", "#4f7fd9", "#6fbf73", "#b07fd9"];

function editableButtons() {
  const list = [];
  if (!macros) return list;
  for (const group of macros.groups) {
    for (const btn of group.buttons) {
      if (btn.editable) list.push({ group: group.name, btn });
    }
  }
  for (const btn of macros.floating) {
    if (btn.editable) list.push({ group: null, btn });
  }
  return list;
}

document.getElementById("macro-add").addEventListener("click", () => {
  const editable = editableButtons();
  if (!editable.length) {
    openMacroEditor(null);
    return;
  }
  openSheet("Macros");
  sheetButton("＋ New button…", () => openMacroEditor(null));
  const header = document.createElement("div");
  header.className = "sheet-header";
  header.textContent = "Edit";
  sheetItems.appendChild(header);
  for (const entry of editable) {
    sheetButton(
      `${entry.btn.label}  (${entry.group || "floating"})`,
      () => openMacroEditor(entry)
    );
  }
});

function openMacroEditor(existing) {
  openSheet(existing ? `Edit: ${existing.btn.label}` : "New macro button");
  const form = document.createElement("form");
  form.className = "sheet-form";

  const labelInput = document.createElement("input");
  labelInput.type = "text";
  labelInput.placeholder = "e.g. Sell gems";
  labelInput.value = existing ? existing.btn.label : "";
  const labelWrap = document.createElement("label");
  labelWrap.append("Label", labelInput);

  const cmdInputEl = document.createElement("input");
  cmdInputEl.type = "text";
  cmdInputEl.placeholder = "e.g. ;sellgems";
  cmdInputEl.autocapitalize = "off";
  cmdInputEl.spellcheck = false;
  cmdInputEl.value = existing ? existing.btn.command || "" : "";
  const cmdWrap = document.createElement("label");
  cmdWrap.append("Command (leave empty for a menu button)", cmdInputEl);

  // Menu options: with any options, tapping the button opens a picker
  // sheet instead of firing a command — "a button that is a category".
  const optionsWrap = document.createElement("div");
  optionsWrap.className = "option-rows";
  const optionRows = [];
  function addOptionRow(label = "", command = "", confirmed = false) {
    const row = document.createElement("div");
    row.className = "option-row";
    const labelIn = document.createElement("input");
    labelIn.type = "text";
    labelIn.placeholder = "option label";
    labelIn.value = label;
    const cmdIn = document.createElement("input");
    cmdIn.type = "text";
    cmdIn.placeholder = "command";
    cmdIn.autocapitalize = "off";
    cmdIn.spellcheck = false;
    cmdIn.value = command;
    const confirmIn = document.createElement("input");
    confirmIn.type = "checkbox";
    confirmIn.checked = confirmed;
    confirmIn.title = "Ask before sending";
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "option-remove";
    remove.textContent = "✕";
    remove.addEventListener("click", () => {
      row.remove();
      optionRows.splice(optionRows.indexOf(entry), 1);
    });
    const entry = { labelIn, cmdIn, confirmIn };
    optionRows.push(entry);
    row.append(labelIn, cmdIn, confirmIn, remove);
    optionsWrap.appendChild(row);
  }
  for (const opt of existing?.btn.options || []) {
    addOptionRow(opt.label, opt.command || "", opt.confirm);
  }
  const addOptionBtn = document.createElement("button");
  addOptionBtn.type = "button";
  addOptionBtn.className = "option-add";
  addOptionBtn.textContent = "＋ Add menu option";
  addOptionBtn.addEventListener("click", () => addOptionRow());
  const optionsLabel = document.createElement("label");
  optionsLabel.append("Menu options", optionsWrap, addOptionBtn);

  // Placement: existing groups, floating, or a new group.
  const placeSelect = document.createElement("select");
  const groupNames = macros ? macros.groups.map((g) => g.name) : [];
  for (const name of groupNames) {
    placeSelect.append(new Option(name, `g:${name}`));
  }
  placeSelect.append(new Option("Floating button", "floating"));
  placeSelect.append(new Option("New group…", "new"));
  const newGroupInput = document.createElement("input");
  newGroupInput.type = "text";
  newGroupInput.placeholder = "group name";
  newGroupInput.hidden = true;
  placeSelect.addEventListener("change", () => {
    newGroupInput.hidden = placeSelect.value !== "new";
  });
  if (existing) {
    placeSelect.value = existing.group ? `g:${existing.group}` : "floating";
  }
  const placeWrap = document.createElement("label");
  placeWrap.append("Show in", placeSelect, newGroupInput);

  // Color presets.
  let chosenColor = existing ? existing.btn.color || null : null;
  const colorRow = document.createElement("div");
  colorRow.className = "form-row";
  const swatches = [];
  for (const color of COLOR_PRESETS) {
    const swatch = document.createElement("button");
    swatch.type = "button";
    swatch.className = "color-swatch";
    if (color) swatch.style.background = color;
    if (color === chosenColor) swatch.classList.add("selected");
    swatch.addEventListener("click", () => {
      chosenColor = color;
      swatches.forEach((s) => s.classList.remove("selected"));
      swatch.classList.add("selected");
    });
    swatches.push(swatch);
    colorRow.appendChild(swatch);
  }

  const confirmToggle = document.createElement("input");
  confirmToggle.type = "checkbox";
  confirmToggle.checked = existing ? !!existing.btn.confirm : false;
  const confirmWrap = document.createElement("label");
  confirmWrap.append(confirmToggle, "Ask before sending");
  const confirmRow = document.createElement("div");
  confirmRow.className = "form-row";
  confirmRow.appendChild(confirmWrap);

  const actions = document.createElement("div");
  actions.className = "form-actions";
  const saveBtn = document.createElement("button");
  saveBtn.type = "submit";
  saveBtn.className = "form-save";
  saveBtn.textContent = "Save";
  actions.appendChild(saveBtn);
  if (existing) {
    const deleteBtn = document.createElement("button");
    deleteBtn.type = "button";
    deleteBtn.className = "form-delete";
    deleteBtn.textContent = "Delete";
    deleteBtn.addEventListener("click", () => {
      closeSheet();
      confirmSheet(`Delete '${existing.btn.label}'`, () => {
        if (!state.ws || state.ws.readyState !== WebSocket.OPEN) return;
        state.ws.send(JSON.stringify({
          t: "macro_delete",
          d: { group: existing.group, label: existing.btn.label },
        }));
      });
    });
    actions.appendChild(deleteBtn);
  }

  form.append(labelWrap, cmdWrap, optionsLabel, placeWrap, colorRow, confirmRow, actions);
  form.addEventListener("submit", (ev) => {
    ev.preventDefault();
    const label = labelInput.value.trim();
    const command = cmdInputEl.value.trim();
    const options = optionRows
      .map((row) => ({
        label: row.labelIn.value.trim(),
        command: row.cmdIn.value.trim(),
        confirm: row.confirmIn.checked,
      }))
      .filter((o) => o.label && o.command);
    if (!label || (!command && !options.length)) return;
    let group = null;
    if (placeSelect.value === "new") {
      group = newGroupInput.value.trim() || null;
      if (!group) return;
    } else if (placeSelect.value.startsWith("g:")) {
      group = placeSelect.value.slice(2);
    }
    if (!state.ws || state.ws.readyState !== WebSocket.OPEN) return;
    state.ws.send(JSON.stringify({
      t: "macro_save",
      d: {
        group,
        label,
        command,
        options,
        color: chosenColor,
        confirm: confirmToggle.checked,
        original: existing ? { group: existing.group, label: existing.btn.label } : null,
      },
    }));
    closeSheet();
  });
  sheetItems.appendChild(form);
}

// ---- Command input, repeat, history ---------------------------------------

const inputForm = document.getElementById("input-row");
const cmdInput = document.getElementById("cmd-input");
const repeatBtn = document.getElementById("repeat-btn");

let cmdHistory = [];
try {
  cmdHistory = JSON.parse(localStorage.getItem(HISTORY_KEY) || "[]");
} catch { /* corrupted storage — start fresh */ }

function recordHistory(text) {
  if (cmdHistory[0] === text) return;
  cmdHistory.unshift(text);
  if (cmdHistory.length > HISTORY_MAX) cmdHistory.length = HISTORY_MAX;
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(cmdHistory));
  } catch { /* storage full/blocked — cmdHistory just won't persist */ }
}

// The echo comes back over the text stream, so nothing renders locally.
inputForm.addEventListener("submit", (ev) => {
  ev.preventDefault();
  const text = cmdInput.value.trim();
  if (!text) return;
  sendCommand(text);
  recordHistory(text);
  cmdInput.value = "";
});

// Repeat button: tap = resend last command; hold = cmdHistory sheet.
let repeatHold = null;
let repeatHeld = false;
repeatBtn.addEventListener("pointerdown", () => {
  repeatHeld = false;
  repeatHold = setTimeout(() => {
    repeatHeld = true;
    openSheet("History");
    if (!cmdHistory.length) {
      sheetNote("No commands yet", true);
      return;
    }
    for (const cmd of cmdHistory) {
      sheetButton(cmd, () => {
        sendCommand(cmd);
        recordHistory(cmd);
      });
    }
  }, 450);
});
repeatBtn.addEventListener("pointerup", () => clearTimeout(repeatHold));
repeatBtn.addEventListener("pointerleave", () => clearTimeout(repeatHold));
repeatBtn.addEventListener("pointercancel", () => clearTimeout(repeatHold));
repeatBtn.addEventListener("contextmenu", (ev) => ev.preventDefault());
repeatBtn.addEventListener("click", () => {
  if (repeatHeld) return; // the hold already opened the cmdHistory sheet
  if (cmdHistory[0]) sendCommand(cmdHistory[0]);
});

// Hardware keyboard: up/down arrows browse cmdHistory in the input field.
let historyIndex = -1;
cmdInput.addEventListener("keydown", (ev) => {
  if (ev.key === "ArrowUp") {
    if (historyIndex < cmdHistory.length - 1) historyIndex += 1;
    if (cmdHistory[historyIndex]) cmdInput.value = cmdHistory[historyIndex];
    ev.preventDefault();
  } else if (ev.key === "ArrowDown") {
    historyIndex -= 1;
    if (historyIndex < 0) {
      historyIndex = -1;
      cmdInput.value = "";
    } else {
      cmdInput.value = cmdHistory[historyIndex] || "";
    }
    ev.preventDefault();
  } else {
    historyIndex = -1;
  }
});

// ---- Text size --------------------------------------------------------------
// Story-text size, adjusted live from a stepper sheet and persisted per
// device (a phone and a tablet want different sizes).

const TEXT_SIZE_KEY = "vellum-text-size";
// 6px is genuinely tiny, but more text on screen beats enforced comfort —
// high-DPI phones keep it legible and it's the user's call (playtest ask).
const TEXT_SIZE_MIN = 6;
const TEXT_SIZE_MAX = 24;
let storySize = parseInt(localStorage.getItem(TEXT_SIZE_KEY) || "14", 10);
if (isNaN(storySize)) storySize = 14;

function applyStorySize() {
  storySize = Math.max(TEXT_SIZE_MIN, Math.min(TEXT_SIZE_MAX, storySize));
  document.documentElement.style.setProperty("--story-size", `${storySize}px`);
  try {
    localStorage.setItem(TEXT_SIZE_KEY, String(storySize));
  } catch { /* fine, just won't persist */ }
  if (autoScroll) scrollToBottom();
}
applyStorySize();

document.getElementById("textsize-btn").addEventListener("click", () => {
  openSheet("Text size");
  const stepper = document.createElement("div");
  stepper.className = "size-stepper";
  const smaller = document.createElement("button");
  smaller.type = "button";
  smaller.textContent = "A−";
  const value = document.createElement("span");
  value.className = "size-value";
  const bigger = document.createElement("button");
  bigger.type = "button";
  bigger.textContent = "A+";
  const refresh = () => {
    value.textContent = `${storySize}px`;
    smaller.disabled = storySize <= TEXT_SIZE_MIN;
    bigger.disabled = storySize >= TEXT_SIZE_MAX;
  };
  smaller.addEventListener("click", () => {
    storySize -= 1;
    applyStorySize();
    refresh();
  });
  bigger.addEventListener("click", () => {
    storySize += 1;
    applyStorySize();
    refresh();
  });
  refresh();
  stepper.append(smaller, value, bigger);
  sheetItems.appendChild(stepper);
  sheetNote("Changes apply live — tap anywhere else to close", true);
});

// ---- Soft keyboard / viewport --------------------------------------------

// Pin the app to the *visual* viewport so the soft keyboard never covers
// the input bar (iOS Safari doesn't resize the layout viewport).
const vv = window.visualViewport;
function syncViewport() {
  if (!vv) return;
  document.documentElement.style.setProperty("--vvh", `${vv.height}px`);
  if (autoScroll) scrollToBottom();
}
if (vv) {
  vv.addEventListener("resize", syncViewport);
  vv.addEventListener("scroll", syncViewport);
  syncViewport();
}

// ---- Screen wake lock ------------------------------------------------------

const wakeBtn = document.getElementById("wake-btn");
let wakeLock = null;
let wakeWanted = false;

async function acquireWakeLock() {
  try {
    wakeLock = await navigator.wakeLock.request("screen");
    wakeLock.addEventListener("release", () => {
      wakeLock = null;
      wakeBtn.classList.toggle("wake-on", wakeWanted);
    });
  } catch {
    wakeWanted = false;
  }
  wakeBtn.classList.toggle("wake-on", wakeWanted);
}

if ("wakeLock" in navigator) {
  wakeBtn.addEventListener("click", () => {
    wakeWanted = !wakeWanted;
    if (wakeWanted) {
      acquireWakeLock();
    } else {
      wakeLock?.release();
      wakeLock = null;
      wakeBtn.classList.remove("wake-on");
    }
  });
  // The lock drops when the tab is backgrounded; re-acquire on return.
  document.addEventListener("visibilitychange", () => {
    if (wakeWanted && document.visibilityState === "visible" && !wakeLock) {
      acquireWakeLock();
    }
  });
} else {
  wakeBtn.hidden = true;
}

// ---- PWA -------------------------------------------------------------------

// Service workers need a secure context; over plain LAN HTTP this silently
// does nothing (Add to Home Screen still works via the manifest).
if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("/sw.js").catch(() => {});
}

// ---- Boot ------------------------------------------------------------------

ensureStream("main");
setActiveStream("main");
setInterval(tickRt, 100);
connect();
