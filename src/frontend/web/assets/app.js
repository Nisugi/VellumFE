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
  inv: "Inventory",
  whisper: "Whispers",
  talk: "Talk",
  speech: "Speech",
  lnet: "LNet",
};
// Streams that never get a prose chip: duplicates of main, or data that
// feeds a dedicated widget/drawer section rather than reading as prose.
// `inv` and the social streams (whisper/talk/speech/lnet) are intentionally
// NOT hidden — they get chips so a phone-only player can read carried items
// and social traffic that host routing might otherwise strand.
const HIDDEN_STREAMS = new Set([
  "room", "spells", "percWindow", "bounty", "society", "assess",
  "experience",
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
let roomId = null;

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

// A vellum://lich?host=…&port=… deep link (QR scan / tapped link) arrives
// from the native shells as #lich=host:port&name=… alongside the boot
// token. Captured here, before loadToken scrubs the fragment; applied to
// the login form once it exists (prefill only — never auto-connect, so a
// malicious QR can't point the app at an attacker's socket unseen).
const bootLich = (() => {
  const m = location.hash.match(/lich=([^&]+)/);
  if (!m) return null;
  // Split on the last colon so bracketed/bare IPv6 hosts survive.
  const target = decodeURIComponent(m[1]);
  const split = target.match(/^(.+):(\d+)$/);
  if (!split) return null;
  const name = location.hash.match(/name=([^&]+)/);
  return {
    host: split[1],
    port: split[2],
    name: name ? decodeURIComponent(name[1]) : "",
  };
})();

// Native-shell markers, also captured before the fragment is scrubbed.
// #app=1 says an iOS/Android shell is hosting this page: only there does
// the Remote login tab exist (its vellum:// actions need a shell to catch
// them), and only there does a non-loopback page offer "back to the app".
const inShell = /(?:^#|&)app=1(?:&|$)/.test(location.hash);

// #remote=host:port — the shell has a remembered remote server (the
// pairing token stays in native secure storage; only the address rides
// the fragment for display).
const shellRemote = (() => {
  const m = location.hash.match(/(?:^#|&)remote=([^&]+)/);
  if (!m) return null;
  const split = decodeURIComponent(m[1]).match(/^(.+):(\d+)$/);
  return split ? { host: split[1], port: split[2] } : null;
})();

// #rhost=…&rport=…[&rkey=…] — a vellum://remote deep link (QR scan on the
// desktop's .webinfo page) relayed by the shell. Prefill only — never
// auto-connect, same reasoning as bootLich. ("rkey", not "rtoken": the
// loadToken regex is unanchored and would swallow any *token= param.)
const bootRemote = (() => {
  const host = location.hash.match(/(?:^#|&)rhost=([^&]+)/);
  const port = location.hash.match(/(?:^#|&)rport=(\d+)/);
  if (!host || !port) return null;
  const key = location.hash.match(/(?:^#|&)rkey=([^&]+)/);
  return {
    host: decodeURIComponent(host[1]),
    port: port[1],
    token: key ? decodeURIComponent(key[1]) : "",
  };
})();

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

// Chip order is user-arrangeable (long-press a chip). It's a roaming
// pref: localStorage is the offline fallback, but a server-side value
// (web.chip_order in the character profile) wins on connect and edits
// are pushed back, so the arrangement follows the character across
// phones. Streams the user hasn't placed keep their first-text arrival
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
  roamPut("web.chip_order", chipOrder.slice());
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
  // Speak before display routing: enabled streams speak even while
  // another stream is active (thoughts read out mid-hunt).
  speakLine(stream, line);
  gpRumbleLine(stream);
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
  roomId = room.id || null;
  roomExits = room.exits || [];
  renderTitle();
  renderCompass();
  // Exits are an interact category; keep its focus in sync.
  if (interact) {
    syncInteractFocus();
    renderInteract();
  }
}

// ---- Compass ----------------------------------------------------------------
// Small always-available rose over the text pane; lit directions are the
// room's exits, tapping one moves. Toggleable in Appearance; shares the
// floating-button opacity.

let roomExits = [];

const COMPASS_CELLS = [
  "nw", "n", "ne", "up",
  "w", "out", "e", "",
  "sw", "s", "se", "down",
];

const EXIT_ALIASES = {
  north: "n", northeast: "ne", east: "e", southeast: "se", south: "s",
  southwest: "sw", west: "w", northwest: "nw", up: "up", down: "down",
  out: "out", u: "up", d: "down",
};

function renderCompass() {
  const compass = document.getElementById("compass");
  const exits = new Set(
    roomExits.map((e) => {
      const key = String(e).toLowerCase().trim();
      return EXIT_ALIASES[key] || key;
    }),
  );
  compass.hidden = exits.size === 0;
  compass.replaceChildren();
  for (const dir of COMPASS_CELLS) {
    const cell = document.createElement("button");
    cell.type = "button";
    cell.className = "compass-cell";
    cell.textContent = dir;
    if (!dir) {
      cell.disabled = true;
      cell.classList.add("compass-blank");
    } else if (exits.has(dir)) {
      cell.classList.add("compass-lit");
      cell.addEventListener("click", () => sendCommand(dir));
    } else {
      cell.disabled = true;
    }
    compass.appendChild(cell);
  }
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
  setInjuries(d.injuries || {});
  setTargets(d.targets || []);
  setRoomEntities(d.entities || {});
  setPortals(d.portals || []);
  setCharInfo(d.char_info || {});
  setRt(d.rt);
  if (d.map_scene) setMapScene(d.map_scene);
  setMapState(d.map_state || {});
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
      // Authenticated: pick up the skin's injury doll art (if any) and
      // any roaming prefs the character profile carries.
      fetchDollSkin();
      fetchRoamingPrefs();
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
    case "wheels":
      wheels = { default: [], named: {}, ...(msg.d || {}) };
      // Input feel + per-wheel aim stick ride along so the phone matches
      // the desktop wheel (one source of truth: host keybinds.toml).
      wheelTuning = { ...WHEEL_TUNING_DEFAULTS, ...(wheels.tuning || {}) };
      wheelStick = wheels.wheel_stick || {};
      wheelStart = wheels.wheel_start || {};
      if (gpWheel) renderWheel();
      break;
    case "session": setSession(msg.d); break;
    case "profiles": renderProfiles(msg.d.list || []); break;
    case "config_file": handleConfigReply(msg.d); break;
    case "sound": playRemoteSound(msg.d); break;
    case "highlights": handleHighlightsReply(msg.d); break;
    case "colors": handleColorsReply(msg.d); break;
    case "settings": handleSettingsReply(msg.d); break;
    case "streams": handleStreamsReply(msg.d); break;
    case "injuries": setInjuries(msg.d); break;
    case "targets": setTargets(msg.d); break;
    case "entities": setRoomEntities(msg.d); break;
    case "portals": setPortals(msg.d); break;
    case "charinfo": setCharInfo(msg.d); break;
    case "map_scene": setMapScene(msg.d); break;
    case "map_state": setMapState(msg.d); break;
    case "map_locations": handleMapLocations(msg.d); break;
    case "map_browse": handleMapBrowse(msg.d); break;
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
  // A connection is already up or in flight (the visibility handler and a
  // pending retry timer can race) — let it finish.
  if (
    state.ws &&
    (state.ws.readyState === WebSocket.CONNECTING ||
      state.ws.readyState === WebSocket.OPEN)
  ) {
    return;
  }
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

// Coming back from the background, the socket is dead and the retry timer
// may be maxed out (10s) — reconnect immediately instead of waiting it out.
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState !== "visible" || authDenied) return;
  state.reconnectDelay = 1000;
  connect();
});

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
  const prevState = session.state;
  session = d || { state: "connected", session_control: false };
  if (session.character) setCharacter(session.character);
  updateSessionUi(prevState);
}

const SESSION_PROGRESS = {
  authenticating: "Authenticating with play.net…",
  connecting: "Connecting to the game…",
};

// States a fresh login passes through on its way to connected. Arriving at
// connected from one of these plays the theme; reconnecting → connected (a
// mid-session drop recovering) and a page load into an already-running
// session (prevState starts out "connected") do not.
const LOGIN_FLOW_STATES = ["idle", "disconnected", "authenticating", "connecting"];

function updateSessionUi(prevState) {
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
    // The classic theme marks the game connection coming up, not the login
    // screen appearing.
    if (LOGIN_FLOW_STATES.includes(prevState)) {
      maybePlayLoginMusic();
      // A headless login just loaded the character's profile config
      // server-side — the hello-time fetch predates it, so re-fetch.
      fetchRoamingPrefs();
    }
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
    detail.textContent = p.mode === "lich"
      ? `${p.name} · Lich @ ${p.host}:${p.port}`
      : `${p.name} · ${p.account_masked} · ${p.game}`;
    btn.append(label, detail);
    btn.addEventListener("click", () => {
      if (p.mode === "lich" || p.has_password) {
        // Lich attaches have no credentials of their own.
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
      if (p.mode !== "lich") {
        sheetNote("The saved password is removed too.", true);
      }
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

// ---- Login mode toggle (play.net direct / Lich attach / remote server) -----

const modeDirectBtn = document.getElementById("mode-direct");
const modeLichBtn = document.getElementById("mode-lich");
const modeRemoteBtn = document.getElementById("mode-remote");
const directFields = document.getElementById("direct-fields");
const lichFields = document.getElementById("lich-fields");
const remoteFields = document.getElementById("remote-fields");
const lichHostInput = document.getElementById("lich-host");
const lichPortInput = document.getElementById("lich-port");
const lichNameInput = document.getElementById("lich-name");
const lichWarning = document.getElementById("lich-warning");
const remoteHostInput = document.getElementById("remote-host");
const remotePortInput = document.getElementById("remote-port");
const remoteTokenInput = document.getElementById("remote-token");
const remoteWarning = document.getElementById("remote-warning");
let loginMode = "direct";

function setLoginMode(mode) {
  loginMode = mode;
  for (const [btn, name] of [
    [modeDirectBtn, "direct"],
    [modeLichBtn, "lich"],
    [modeRemoteBtn, "remote"],
  ]) {
    btn.classList.toggle("mode-active", mode === name);
    btn.setAttribute("aria-selected", String(mode === name));
  }
  directFields.hidden = mode !== "direct";
  lichFields.hidden = mode !== "lich";
  remoteFields.hidden = mode !== "remote";
}
modeDirectBtn.addEventListener("click", () => setLoginMode("direct"));
modeLichBtn.addEventListener("click", () => setLoginMode("lich"));
modeRemoteBtn.addEventListener("click", () => setLoginMode("remote"));
modeRemoteBtn.hidden = !inShell;

// Deep-linked Lich target: open the Lich tab prefilled and let the user
// press Connect.
if (bootLich) {
  lichHostInput.value = bootLich.host;
  lichPortInput.value = bootLich.port;
  lichNameInput.value = bootLich.name;
  lichHostInput.dispatchEvent(new Event("input"));
  setLoginMode("lich");
}

// Remembered remote server: one-tap row above the manual fields. Connect
// and Forget are shell actions — the token never reaches this page.
if (shellRemote && inShell) {
  const row = document.getElementById("remote-saved");
  document.getElementById("remote-saved-name").textContent = "Saved server";
  document.getElementById("remote-saved-detail").textContent =
    `${shellRemote.host}:${shellRemote.port}`;
  document.getElementById("remote-saved-connect").addEventListener("click", () => {
    location.href = "vellum://remote/connect";
  });
  document.getElementById("remote-saved-forget").addEventListener("click", () => {
    openSheet(`Forget server ${shellRemote.host}:${shellRemote.port}?`);
    sheetButton("Forget", () => {
      location.href = "vellum://remote/forget";
    });
    sheetNote("The stored pairing token is removed too.", true);
  });
  row.hidden = false;
}

// Deep-linked remote server (.webinfo app QR): open the Remote tab
// prefilled and let the user press Connect.
if (bootRemote && inShell) {
  remoteHostInput.value = bootRemote.host;
  remotePortInput.value = bootRemote.port;
  remoteTokenInput.value = bootRemote.token;
  remoteHostInput.dispatchEvent(new Event("input"));
  setLoginMode("remote");
}

// The Lich port is unauthenticated: nudge (don't block) when the target
// doesn't look like loopback / RFC1918 / Tailscale CGNAT / mDNS / tailnet.
// Non-IP hostnames can't be classified — assume the user knows their DNS.
function lichHostLooksPrivate(host) {
  const h = host.toLowerCase();
  if (h === "localhost" || h === "::1" || h.endsWith(".local") || h.endsWith(".ts.net")) {
    return true;
  }
  const v4 = h.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/);
  if (!v4) return !h.includes(":") || h.startsWith("fd") || h.startsWith("fe80");
  const a = Number(v4[1]);
  const b = Number(v4[2]);
  if (a === 10 || a === 127) return true;
  if (a === 192 && b === 168) return true;
  if (a === 172 && b >= 16 && b <= 31) return true;
  if (a === 100 && b >= 64 && b <= 127) return true; // Tailscale (CGNAT)
  return false;
}

lichHostInput.addEventListener("input", () => {
  const host = lichHostInput.value.trim();
  const risky = host !== "" && !lichHostLooksPrivate(host);
  lichWarning.textContent = risky
    ? "This address looks public. The Lich connection has no password — reach your PC over a VPN (Tailscale/WireGuard) instead of the open internet."
    : "";
  lichWarning.hidden = !risky;
});

remoteHostInput.addEventListener("input", () => {
  const host = remoteHostInput.value.trim();
  const risky = host !== "" && !lichHostLooksPrivate(host);
  remoteWarning.textContent = risky
    ? "This address looks public. Reach your PC over a VPN (Tailscale/WireGuard) instead of exposing the web port to the open internet."
    : "";
  remoteWarning.hidden = !risky;
});

sessionForm.addEventListener("submit", (ev) => {
  ev.preventDefault();
  if (loginMode === "remote") {
    // Hand the target to the native shell: it stores the pairing (Keychain/
    // Keystore), admits the host in its WebView allowlist, and points the
    // WebView at that server's dashboard. The embedded core stays idle.
    const host = remoteHostInput.value.trim();
    const port = remotePortInput.value.trim();
    const token = remoteTokenInput.value.trim();
    if (!host || !/^\d+$/.test(port)) return;
    const save = document.getElementById("remote-save").checked;
    let target = `vellum://remote?host=${encodeURIComponent(host)}&port=${port}`;
    if (token) target += `&token=${encodeURIComponent(token)}`;
    if (!save) target += "&save=0";
    location.href = target;
    return;
  }
  if (loginMode === "lich") {
    const host = lichHostInput.value.trim();
    const port = lichPortInput.value.trim();
    const name = lichNameInput.value.trim();
    if (!host || !/^\d+$/.test(port)) return;
    const save = document.getElementById("lich-save").checked;
    sendJson("connect", {
      mode: "lich",
      host,
      port,
      character: name || null,
      profile_name: save ? (name || `${host}:${port}`) : null,
    });
    profilesRequested = false;
    return;
  }
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

// ---- Speech (text-to-speech) ----------------------------------------------
// Browser speechSynthesis reads incoming lines aloud — the phone's
// accessibility story until native TTS bridges exist. Chronological and
// non-interrupting by design: the browser queues utterances natively.
// Prefs are per-device (localStorage), mirroring the desktop defaults
// (thoughts on, main off).

const SPEECH_PREFS_KEY = "vellum-speech";
let speechPrefs = { enabled: false, rate: 1.0, voice: "", streams: {} };
try {
  const stored = JSON.parse(localStorage.getItem(SPEECH_PREFS_KEY) || "{}");
  speechPrefs = { ...speechPrefs, ...stored };
  speechPrefs.streams = { thoughts: true, ...(stored.streams || {}) };
} catch { /* corrupted storage — defaults */ }

function saveSpeechPrefs() {
  try {
    localStorage.setItem(SPEECH_PREFS_KEY, JSON.stringify(speechPrefs));
  } catch { /* private mode */ }
}

function speechAvailable() {
  return "speechSynthesis" in window;
}

// Utterances in flight/queued. Under a firehose we drop new lines past a
// cap instead of drifting minutes behind the game.
let speechQueued = 0;
const SPEECH_QUEUE_CAP = 40;

function speechVoiceByName(name) {
  if (!name || !speechAvailable()) return null;
  return speechSynthesis.getVoices().find((v) => v.name === name) || null;
}

function speakLine(stream, line) {
  if (!speechPrefs.enabled || !speechAvailable()) return;
  if (!speechPrefs.streams[stream]) return;
  const text = (line.segments || [])
    .map((seg) => seg.text || "")
    .join("")
    .trim();
  if (text.length <= 1) return; // prompts and blanks
  if (speechQueued >= SPEECH_QUEUE_CAP) return;
  const utterance = new SpeechSynthesisUtterance(text);
  utterance.rate = Math.min(Math.max(speechPrefs.rate || 1.0, 0.5), 3.0);
  const voice = speechVoiceByName(speechPrefs.voice);
  if (voice) utterance.voice = voice;
  speechQueued += 1;
  const done = () => {
    speechQueued = Math.max(0, speechQueued - 1);
  };
  utterance.onend = done;
  utterance.onerror = done;
  speechSynthesis.speak(utterance);
}

function stopSpeaking() {
  if (!speechAvailable()) return;
  speechSynthesis.cancel();
  speechQueued = 0;
}

// Streams offered in the speech sheet: every visible chip stream seen this
// session plus the well-known ones, so thoughts can be enabled before the
// first thought arrives.
function speechStreamChoices() {
  const known = new Set(["main", "thoughts", "familiar", "death", "logons"]);
  for (const stream of buffers.keys()) known.add(stream);
  return [...known].filter((stream) => !HIDDEN_STREAMS.has(stream));
}

// ---- Controller (browser Gamepad API) --------------------------------------
// Bluetooth/USB pads (Backbone, Kishi, Xbox, PlayStation, MFi) with the
// standard mapping. Buttons map to commands — defaults mirror the desktop
// [controller] table — and while any bottom sheet is open the d-pad
// navigates it: up/down move focus, South taps, East closes. There is no
// mapper software on phones, so this is the only controller path here.
// Prefs are per-device (localStorage): pads travel with the phone.
//
// Beyond plain commands, a bind can be a reserved word: "shift" (hold
// modifier), "wheel" / "wheel:<name>" (hold-open radial command wheel —
// definitions come from the host's keybinds.toml via the `wheels`
// message; picks resolve server-side like macro taps), or a client-side
// UI action from GP_UI_ACTIONS (left_panel, right_panel, map, ...).

const CONTROLLER_PREFS_KEY = "vellum-controller";
const GP_BUTTON_NAMES = {
  0: "south", 1: "east", 2: "west", 3: "north",
  4: "l1", 5: "r1", 6: "l2", 7: "r2",
  8: "select", 9: "start", 10: "l3", 11: "r3",
  12: "dpad_up", 13: "dpad_down", 14: "dpad_left", 15: "dpad_right",
  16: "guide",
};
const GP_BUTTON_ORDER = [
  "dpad_up", "dpad_down", "dpad_left", "dpad_right",
  "south", "east", "west", "north",
  "l1", "r1", "l2", "r2", "l3", "r3",
  "select", "start", "guide",
];
// The left stick walks the 8 compass directions; the d-pad covers the
// vertical axis and portals (.portal resolves go door / climb stair on
// the host). Binding a button to the reserved word "shift" makes it a
// hold-modifier: while held, buttons resolve against shiftBinds.
const GP_SHIFT = "shift";
const GP_DEFAULT_BINDS = {
  dpad_up: "up", dpad_down: "down", dpad_right: "out", dpad_left: ".portal",
  south: "look", l2: GP_SHIFT, r2: "wheel",
  // r3, not l3: wheels aim with the LEFT stick, and clicking that same
  // stick deflects it into a stray compass move before the hold lands.
  r3: "wheel:portals",
  start: "interact",
};
const GP_DEFAULT_SHIFT_BINDS = { south: "stand" };

let controllerPrefs = {
  enabled: true,
  binds: { ...GP_DEFAULT_BINDS },
  shiftBinds: { ...GP_DEFAULT_SHIFT_BINDS },
};
try {
  const stored = JSON.parse(localStorage.getItem(CONTROLLER_PREFS_KEY) || "{}");
  controllerPrefs = { ...controllerPrefs, ...stored };
  if (!stored.binds) controllerPrefs.binds = { ...GP_DEFAULT_BINDS };
  if (!stored.shiftBinds) controllerPrefs.shiftBinds = { ...GP_DEFAULT_SHIFT_BINDS };
} catch { /* corrupted storage — defaults */ }

function saveControllerPrefs() {
  try {
    localStorage.setItem(CONTROLLER_PREFS_KEY, JSON.stringify(controllerPrefs));
  } catch { /* private mode */ }
}

let gpLoop = 0;
const gpPrev = new Map(); // pad.index -> [pressed]
let gpFocusIndex = -1; // d-pad focus among sheet items
let gpEditButton = null; // button row expanded in the controller sheet

function gamepadPads() {
  try {
    return [...(navigator.getGamepads?.() || [])].filter(Boolean);
  } catch {
    return [];
  }
}

function startGamepadLoop() {
  if (!gpLoop) gpLoop = setInterval(pollGamepads, 33);
}

function stopGamepadLoop() {
  clearInterval(gpLoop);
  gpLoop = 0;
  gpPrev.clear();
}

window.addEventListener("gamepadconnected", () => {
  startGamepadLoop();
  if (controllerSheetActive()) renderControllerSheet();
});
window.addEventListener("gamepaddisconnected", () => {
  if (!gamepadPads().length) stopGamepadLoop();
  if (controllerSheetActive()) renderControllerSheet();
});

// Left stick: 8-way compass movement. One command per deflection into a
// 45-degree sector (clockwise from north), re-armed by returning toward
// center — hysteresis keeps a wobbling stick from spamming moves.
const GP_STICK_DIRS = ["n", "ne", "e", "se", "s", "sw", "w", "nw"];
let gpStickSector = null;

function gpStickSectorFor(x, yUp, previous) {
  const magnitude = Math.hypot(x, yUp);
  if (magnitude < (previous !== null ? 0.35 : 0.6)) return null;
  const angle = (Math.atan2(x, yUp) * 180) / Math.PI; // 0 = north, cw
  return Math.floor(((angle + 382.5) % 360) / 45) % 8;
}

// ---- Radial command wheel (hold a wheel-bound button, aim, release) --------
// The phone counterpart of the desktop wheel: definitions arrive from the
// host in the `wheels` message (labels/colors/folders only — commands
// stay server-side and picks resolve there, like macro taps). While the
// wheel is up it owns the pad: the left stick aims, South opens a folder
// (or fires a leaf), East backs up a level, release fires the aimed leaf.

let wheels = { default: [], named: {} };
// Input-feel tuning + per-wheel aim stick, pushed by the host in the
// `wheels` message (mirrors [controller_tuning] / per-wheel stick). The
// defaults reproduce the historical feel until the host pushes real
// values.
const WHEEL_TUNING_DEFAULTS = {
  movement_stick: "left",
  back_slice: "down",
  deadzone: 50,
  aim_dwell_ms: 150,
  nav_dwell_ms: 150,
  fire_debounce_ms: 300,
  release_grace_ms: 40,
  fire_mode: "release",
  edge_threshold: 90,
  retract_delta: 10,
};
let wheelTuning = { ...WHEEL_TUNING_DEFAULTS };
let wheelStick = {}; // wheel name ("default" = default wheel) -> "left"|"right"
let wheelStart = {}; // wheel name -> ring rotation in degrees (0 = up)
// Dwell wheel state. `aimed` is the committed display slice (release
// fires it); `candidate`/`candidateSince` track the slice the stick is
// over but hasn't dwelt on yet; `rearmUntilCenter` blocks a new dwell
// until the stick re-neutralizes after an auto descend/ascend (so a
// still-deflected stick can't chain through folders). Indices are into
// the DISPLAYED ring (Back slice injected inside folders).
let gpWheel = null; // { key, path, aimed, candidate, candidateSince, rearmUntilCenter }
// A leaf already fired during this hold of the wheel button; the wheel
// stays closed (and the stick stays quiet) until a fresh hold, so one
// hold never fires twice or walks on release.
let gpWheelFired = false;
// After a wheel closes with the aim stick still deflected, its normal
// function (scroll / interact cycle) stays suppressed until it recenters.
let gpAimRecenterNeeded = false;
// Sentinel command/marker for the injected Back slice (wheel-core.js).
const WHEEL_BACK = WheelCore.WHEEL_BACK;

// The wheel key of a bind value: "" for "wheel", the name for
// "wheel:<name>", null for anything that is not a wheel bind.
function gpWheelKeyOf(bind) {
  if (bind === "wheel") return "";
  if (typeof bind === "string" && bind.startsWith("wheel:")) return bind.slice(6);
  return null;
}

// The wheel key of the wheel-bound button currently held, if any — read
// from live pad state like the shift modifier (base layer only).
function gpHeldWheelKey() {
  for (const [index, name] of Object.entries(GP_BUTTON_NAMES)) {
    const key = gpWheelKeyOf(controllerPrefs.binds[name]);
    if (key === null) continue;
    const i = Number(index);
    if (gamepadPads().some((pad) => !!(pad.buttons[i] && pad.buttons[i].pressed))) {
      return key;
    }
  }
  return null;
}

// The room's portal commands ("go arch"), pushed by the host for the
// dynamic "portals" wheel; picks resolve server-side by index.
let portalCommands = [];

function setPortals(portals) {
  portalCommands = portals || [];
  if (gpWheel && gpWheel.key === "portals") renderWheel();
}

// The slice list at a folder path within a wheel; null when the key or
// path no longer resolves (stale after a host-side wheel edit). The
// reserved "portals" wheel is built from the live portal list — flat,
// label = the command minus its verb ("go gate" reads as "gate").
function wheelLevelSlices(key, path) {
  if (key === "portals") {
    if (path.length) return null;
    return portalCommands.map((command) => ({
      label: command.includes(" ")
        ? command.slice(command.indexOf(" ") + 1)
        : command,
    }));
  }
  let level = key ? (wheels.named || {})[key] : wheels.default;
  if (!Array.isArray(level)) return null;
  for (const index of path) {
    level = (level[index] || {}).slices;
    if (!Array.isArray(level)) return null;
  }
  return level;
}

// Wheel dead zone as a 0..1 stick magnitude (percent in tuning).
function wheelDeadzone() {
  return Math.min(0.99, Math.max(0, (wheelTuning.deadzone || 0) / 100));
}

// The displayed ring for a wheel level: gathers this level's slices from
// state and delegates the geometry (Back injection + anchor rotation) to
// WheelCore.buildWheelView — the shared, node-tested core.
function wheelView(key, path) {
  const real = wheelLevelSlices(key, path);
  if (!Array.isArray(real)) return null;
  const start = wheelStart[key === "" ? "default" : key] || 0;
  return WheelCore.buildWheelView(real, path.length > 0, wheelTuning.back_slice, start);
}

function sendWheelPick(key, path) {
  if (!state.ws || state.ws.readyState !== WebSocket.OPEN) return;
  state.ws.send(JSON.stringify({ t: "wheel_pick", d: { key, path } }));
}

const wheelOverlay = document.getElementById("wheel-overlay");
const WHEEL_SVG_NS = "http://www.w3.org/2000/svg";

function wheelSvgEl(tag, attrs, style) {
  const el = document.createElementNS(WHEEL_SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  // Fills/strokes go through style so CSS variables resolve.
  for (const [k, v] of Object.entries(style || {})) el.style[k] = v;
  return el;
}

function hideWheel() {
  wheelOverlay.hidden = true;
  wheelOverlay.replaceChildren();
}

// Draw the current wheel level: a ring of wedges around the screen
// center, the aimed slice highlighted; tinted wedges dim at rest and
// brighten while aimed (matches the desktop wheel).
function renderWheel() {
  const view = gpWheel && wheelView(gpWheel.key, gpWheel.path);
  const slices = view && view.slices;
  if (!slices || !slices.length) {
    hideWheel();
    return;
  }
  const { aimed } = gpWheel;
  const inFolder = gpWheel.path.length > 0;
  const outer = 104;
  const hub = 34;
  const labelR = 70;
  // Per-seat angles from the resolved layout (aim degrees, 0 = up cw);
  // screen radians = deg*pi/180 - pi/2 (up is -90 in screen space).
  const seats = view.layout.seats;
  const seatCenter = (i) => (seats[i].startDeg + seats[i].spanDeg / 2) * (Math.PI / 180) - Math.PI / 2;
  const seatSpan = (i) => seats[i].spanDeg * (Math.PI / 180);
  const globalDz = wheelDeadzone();

  const svg = wheelSvgEl("svg", { viewBox: "-110 -110 220 220" });
  svg.appendChild(
    wheelSvgEl("circle", { cx: 0, cy: 0, r: outer, "fill-opacity": 0.92 }, {
      fill: "var(--bg-panel)",
      stroke: "var(--border)",
    }),
  );

  const point = (angle, radius) =>
    `${(Math.cos(angle) * radius).toFixed(2)} ${(Math.sin(angle) * radius).toFixed(2)}`;

  slices.forEach((slice, i) => {
    const centerAngle = seatCenter(i);
    const step = seatSpan(i);
    const isAimed = aimed === i;
    // Wedge fill: the slice's tint (dim at rest, bright while aimed);
    // colorless slices highlight with the accent color.
    const fill = slice.color || (isAimed ? "var(--accent, #6a9fb5)" : null);
    if (fill) {
      const opacity = slice.color ? (isAimed ? 0.85 : 0.22) : 0.5;
      const wedge =
        slices.length === 1
          ? wheelSvgEl("circle", { cx: 0, cy: 0, r: outer, "fill-opacity": opacity }, { fill })
          : wheelSvgEl(
              "path",
              {
                d:
                  `M 0 0 L ${point(centerAngle - step / 2, outer)} ` +
                  `A ${outer} ${outer} 0 0 1 ${point(centerAngle + step / 2, outer)} Z`,
                "fill-opacity": opacity,
              },
              { fill },
            );
      svg.appendChild(wedge);
    }
    if (slices.length > 1) {
      const boundary = centerAngle - step / 2;
      svg.appendChild(
        wheelSvgEl(
          "line",
          {
            x1: Math.cos(boundary) * hub,
            y1: Math.sin(boundary) * hub,
            x2: Math.cos(boundary) * outer,
            y2: Math.sin(boundary) * outer,
          },
          { stroke: "var(--border)" },
        ),
      );
    }
    const isFolder = !!(slice.slices || []).length;
    let label = `${slice.label || ""}${isFolder ? " ▸" : ""}`;
    if (label.length > 16) label = `${label.slice(0, 15)}…`;
    const text = wheelSvgEl(
      "text",
      {
        x: Math.cos(centerAngle) * labelR,
        y: Math.sin(centerAngle) * labelR,
        "text-anchor": "middle",
        "dominant-baseline": "middle",
        "font-size": isAimed ? 13 : 10,
        "font-weight": isAimed ? "bold" : "normal",
      },
      { fill: isAimed ? "var(--fg)" : "var(--fg-dim)" },
    );
    text.textContent = label;
    svg.appendChild(text);

    // Per-slice inner floor: a slice whose own `inner` sits above the
    // global dead zone needs a deeper throw — draw a faint arc across its
    // wedge at that floor radius (mirrors the desktop cue).
    if (slice.inner != null) {
      const frac = Math.min(1, Math.max(0, slice.inner / 100));
      if (frac > globalDz + 1e-3) {
        const r = hub + (outer - hub) * frac;
        const a0 = centerAngle - step / 2;
        const a1 = centerAngle + step / 2;
        svg.appendChild(
          wheelSvgEl(
            "path",
            {
              d: `M ${point(a0, r)} A ${r} ${r} 0 0 1 ${point(a1, r)}`,
              fill: "none",
            },
            { stroke: "var(--warn, #d0a020)", "stroke-width": 1.5, opacity: 0.8 },
          ),
        );
      }
    }
  });

  // Center hub hosts the hint text.
  svg.appendChild(
    wheelSvgEl("circle", { cx: 0, cy: 0, r: hub }, {
      fill: "var(--bg-panel)",
      stroke: "var(--border)",
    }),
  );
  const aimedSlice = aimed !== null ? slices[aimed] : null;
  const hint = aimedSlice
    ? WheelCore.isBackSlice(aimedSlice)
      ? "dwell to go back"
      : (aimedSlice.slices || []).length
        ? "dwell to open"
        : "release to fire"
    : inFolder
      ? "dwell · release fires · center to cancel"
      : "aim, dwell, release to fire";
  const hintText = wheelSvgEl(
    "text",
    {
      x: 0,
      y: 0,
      "text-anchor": "middle",
      "dominant-baseline": "middle",
      "font-size": 8,
    },
    { fill: "var(--fg-dim)" },
  );
  hintText.textContent = hint;
  svg.appendChild(hintText);

  wheelOverlay.replaceChildren(svg);
  wheelOverlay.hidden = false;
}

// ---- Controller UI actions -------------------------------------------------
// Client-side panel toggles bindable to buttons alongside game commands:
// the phone UI tucks everything into drawers/sheets/overlays, and a pad
// user needs buttons to reach them. Sheets opened this way inherit the
// d-pad navigation (up/down move, A taps, B closes).
const GP_UI_ACTIONS = {
  left_panel: () => {
    if (drawerLeft.classList.contains("open")) closeDrawers();
    else openDrawer("left");
  },
  right_panel: () => {
    if (drawerRight.classList.contains("open")) closeDrawers();
    else openDrawer("right");
  },
  map: () => (mapOverlay.hidden ? openMapOverlay() : closeMapOverlay()),
  interact: () => toggleInteract(),
  effects: () => openEffectsSheet(),
  settings: () => openSettingsSheet(),
  appearance: () => openAppearanceSheet(),
  speech: () => openSpeechSheet(),
  controller: () => openControllerSheet(),
};

// Dispatch a bind value: reserved words stay local (shift/wheel are
// hold-modifiers handled elsewhere), UI actions run client-side, and
// anything else is a game command (or dot-command) for the host —
// with <target_id>/<target_noun> filled from the interact focus, so a
// bound "target #<target_id>\rincant 611" attacks whatever the focus
// ring is on.
function gpDispatchBind(bind) {
  if (!bind || bind === GP_SHIFT || gpWheelKeyOf(bind) !== null) return;
  const action = GP_UI_ACTIONS[bind];
  if (action) return action();
  const resolved = substituteInteractPlaceholders(bind);
  if (resolved === null) {
    flashInteractNote("macro needs an interact target");
    return;
  }
  sendCommand(resolved);
}

// Fill <target_id>/<target_noun> from the focused interact entity; text
// without placeholders passes through untouched. Null = the macro needs
// a target but none is focused (mode off, empty category, or an exit) —
// drop it rather than send the literal placeholder.
function substituteInteractPlaceholders(text) {
  if (!text.includes("<target_id>") && !text.includes("<target_noun>")) {
    return text;
  }
  const current = interactCurrent();
  const entity = current && current.entity;
  if (!entity || entity.command) return null; // exits have no exist id
  return text
    .replaceAll("<target_id>", entity.existId)
    .replaceAll("<target_noun>", entity.noun || "");
}

// ---- Interact mode ---------------------------------------------------------
// The phone port of the desktop focus cycle (core/app_core/interact.rs):
// walk the room's Creatures / Objects / Players / Exits without touching
// the screen or rendering a room window. Entity lists arrive in the
// snapshot and `entities` deltas (exits ride the room payload);
// activation is the ordinary link-tap round-trip, so the server menu
// opens as the usual bottom sheet — which the d-pad already navigates.
// Toggle by the ◎ button (touch) or a button bound to "interact" (pad,
// Start by default); up/down cycle entities, left/right categories.
// Focus is remembered by exist id so room churn can't steal it.

const INTERACT_ORDER = ["creatures", "objects", "players", "exits"];
const INTERACT_LABELS = {
  creatures: "Creatures", objects: "Objects", players: "Players", exits: "Exits",
};
// Compass short code -> movement command word (full words pass through).
const EXIT_COMMANDS = {
  n: "north", ne: "northeast", e: "east", se: "southeast",
  s: "south", sw: "southwest", w: "west", nw: "northwest",
};

let roomEntities = { creatures: [], objects: [], players: [] };
let interact = null; // { category, index, focusKey }

function setRoomEntities(entities) {
  roomEntities = {
    creatures: entities.creatures || [],
    objects: entities.objects || [],
    players: entities.players || [],
  };
  if (interact) {
    syncInteractFocus();
    renderInteract();
  }
  // The status drawer shows a Players section fed from here.
  if (drawerRight.classList.contains("open")) renderStatusDrawer();
}

/// Entities for one category, in display order: { key, label } plus
/// either { existId, noun } (menu entities) or { command } (exits).
function interactEntities(category) {
  if (category === "exits") {
    return roomExits.map((dir) => {
      const command = EXIT_COMMANDS[dir] || dir;
      return { key: dir, label: command, command };
    });
  }
  return (roomEntities[category] || []).map((e) => ({
    key: e.id, label: e.label, existId: e.id, noun: e.noun,
  }));
}

function toggleInteract() {
  if (interact) exitInteract();
  else enterInteract();
}

function enterInteract() {
  const category = INTERACT_ORDER.find((c) => interactEntities(c).length);
  if (!category) {
    flashInteractNote("nothing here to interact with");
    return;
  }
  interact = { category, index: 0, focusKey: null };
  syncInteractFocus();
  renderInteract();
  announceInteractFocus();
}

function exitInteract() {
  interact = null;
  interactBar.hidden = true;
}

// Re-resolve the stored focus against current lists: find the remembered
// key, else clamp the index — room churn between deltas can't leave a
// stale index.
function syncInteractFocus() {
  if (!interact) return;
  const entities = interactEntities(interact.category);
  if (!entities.length) {
    interact.index = 0;
    interact.focusKey = null;
    return;
  }
  const found = interact.focusKey === null
    ? -1
    : entities.findIndex((e) => e.key === interact.focusKey);
  if (found >= 0) {
    interact.index = found;
  } else {
    interact.index = Math.min(interact.index, entities.length - 1);
    interact.focusKey = entities[interact.index].key;
  }
}

function interactCurrent() {
  if (!interact) return null;
  const entities = interactEntities(interact.category);
  const entity = entities[interact.index];
  return entity ? { category: interact.category, index: interact.index, count: entities.length, entity } : null;
}

/// Move focus within the category (+1 / -1), wrapping.
function interactMove(delta) {
  if (!interact) return;
  syncInteractFocus();
  const entities = interactEntities(interact.category);
  if (!entities.length) return renderInteract();
  interact.index = (interact.index + delta + entities.length) % entities.length;
  interact.focusKey = entities[interact.index].key;
  renderInteract();
  announceInteractFocus();
}

/// Switch category (+1 / -1), skipping empty ones, wrapping.
function interactCategoryMove(delta) {
  if (!interact) return;
  const start = INTERACT_ORDER.indexOf(interact.category);
  const len = INTERACT_ORDER.length;
  for (let step = 1; step <= len; step++) {
    const candidate = INTERACT_ORDER[(start + delta * step + len * len) % len];
    if (candidate === interact.category) continue;
    if (interactEntities(candidate).length) {
      interact.category = candidate;
      interact.index = 0;
      interact.focusKey = null;
      syncInteractFocus();
      renderInteract();
      announceInteractFocus();
      return;
    }
  }
}

/// Activate the focused entity: the server context menu for entities
/// (same link-tap round-trip as tapping a name in the story), a bare
/// movement command for exits — walking leaves the room, so the mode
/// drops with it.
function interactActivate() {
  syncInteractFocus();
  const current = interactCurrent();
  if (!current) return;
  const { entity } = current;
  if (entity.command) {
    exitInteract();
    sendCommand(entity.command);
    return;
  }
  if (!state.ws || state.ws.readyState !== WebSocket.OPEN) return;
  const requestId = ++menuRequestCounter;
  pendingMenuRequest = requestId;
  openSheetLoading(entity.noun || entity.label);
  state.ws.send(JSON.stringify({
    t: "link_tap",
    d: {
      request_id: requestId,
      exist_id: entity.existId,
      noun: entity.noun || "",
      text: entity.label,
    },
  }));
}

const interactBar = document.getElementById("interact-bar");
let interactNoteTimer = 0;

// Interact-bar placement (persisted per device in uiPrefs.interactPos):
//   { mode: "dock", edge: "top"|"bottom" }        full-width along an edge
//   { mode: "float", x: 0..1, y: 0..1, w: px }    free-floating, dropped
// Default is a bottom dock (the historical position). Long-pressing the
// bar drags it; dropping near the top/bottom edge snaps to a dock, else
// it free-floats where dropped. Taps on the buttons still change target.
function defaultInteractPos() {
  return { mode: "dock", edge: "bottom" };
}

function applyInteractPos() {
  const p = (uiPrefs.interactPos && uiPrefs.interactPos.mode)
    ? uiPrefs.interactPos
    : defaultInteractPos();
  const s = interactBar.style;
  if (p.mode === "float") {
    // A floating bar is a compact, content-width puck (CSS .floating sets
    // width:max-content) so it can move in both axes; docked stays full
    // width for easy tapping. Clamp the anchor into the viewport.
    interactBar.classList.add("floating");
    s.width = "";
    s.right = "auto";
    s.bottom = "auto";
    s.left = `${Math.min(0.98, Math.max(0.02, p.x != null ? p.x : 0.5)) * 100}%`;
    s.top = `${Math.min(0.95, Math.max(0.02, p.y != null ? p.y : 0.85)) * 100}%`;
  } else {
    interactBar.classList.remove("floating");
    s.width = "";
    s.left = "8px";
    s.right = "8px";
    if (p.edge === "top") {
      s.top = "8px";
      s.bottom = "auto";
    } else {
      s.bottom = "8px";
      s.top = "auto";
    }
  }
}

// Long-press to drag; a plain tap falls through to the buttons. Mirrors
// the floating-button gesture (hold timer + movement threshold + capture),
// then snaps to an edge dock or free-floats on drop.
(function attachInteractDrag() {
  // Drag is MOVEMENT-driven, not hold-driven: a tap (down+up with no real
  // movement) falls through to the buttons/target-select; moving the
  // finger past MOVE_EPS starts a drag. (An earlier 300ms hold gate made
  // a natural quick grab-and-drag register as a tap — the bar never moved,
  // which read as "stuck".)
  const MOVE_EPS = 8; // px of movement before a press becomes a drag
  const EDGE_PX = 64; // finger within this of a container edge on release = dock
  let tracking = false; // pointer is down on the bar, watching for movement
  let dragging = false;
  let pointerId = null;
  let startX = 0, startY = 0;
  let lastPointerY = 0; // live finger Y, for edge-distance snap on release

  const parentRect = () => interactBar.offsetParent
    ? interactBar.offsetParent.getBoundingClientRect()
    : { left: 0, top: 0, width: window.innerWidth, height: window.innerHeight };

  interactBar.addEventListener("pointerdown", (ev) => {
    tracking = true;
    dragging = false;
    pointerId = ev.pointerId;
    startX = ev.clientX;
    startY = ev.clientY;
    lastPointerY = ev.clientY;
  });

  interactBar.addEventListener("pointermove", (ev) => {
    if (!tracking && !dragging) return;
    lastPointerY = ev.clientY;
    const dx = ev.clientX - startX;
    const dy = ev.clientY - startY;
    if (!dragging) {
      if (Math.hypot(dx, dy) < MOVE_EPS) return; // not enough movement = still a tap
      dragging = true;
      // .floating shrinks the bar to content width (a movable puck) so it
      // tracks the finger in both axes instead of staying full-width.
      interactBar.classList.add("dragging", "floating");
      try { interactBar.setPointerCapture(ev.pointerId); } catch { /* ok */ }
      // Clear the docked anchors so only `top` drives vertical position.
      // Leaving `bottom` (or `right`) set alongside `top`/`left` makes CSS
      // stretch the bar to span between both anchors during the drag.
      interactBar.style.width = "";
      interactBar.style.right = "auto";
      interactBar.style.bottom = "auto";
    }
    const r = parentRect();
    const w = interactBar.offsetWidth;
    const h = interactBar.offsetHeight;
    // Anchor by the bar's top-left, keeping the grab point under the finger.
    let x = (ev.clientX - r.left - w / 2) / r.width;
    let y = (ev.clientY - r.top - h / 2) / r.height;
    x = Math.min(0.98, Math.max(0.02, x));
    y = Math.min(0.95, Math.max(0.02, y));
    interactBar.style.left = `${x * 100}%`;
    interactBar.style.top = `${y * 100}%`;
    interactBar.dataset.dx = x;
    interactBar.dataset.dy = y;
  });

  const endDrag = (ev) => {
    tracking = false;
    pointerId = null;
    if (dragging) {
      // Snap by the finger's distance from an edge of the bar's OWN
      // positioning container (#pane-wrap), not the window — the bar sits
      // below the header/chips, so window.innerHeight math was off and a
      // bottom-docked bar kept re-docking. Measure release-Y inside the
      // container; dock only within EDGE_PX of its top/bottom edge.
      const r = parentRect();
      // Prefer the release event's own Y; fall back to the last move.
      const rawY = (ev && typeof ev.clientY === "number") ? ev.clientY : lastPointerY;
      const relY = rawY - r.top; // finger Y within the container
      if (relY <= EDGE_PX) {
        uiPrefs.interactPos = { mode: "dock", edge: "top" };
      } else if (relY >= r.height - EDGE_PX) {
        uiPrefs.interactPos = { mode: "dock", edge: "bottom" };
      } else {
        uiPrefs.interactPos = {
          mode: "float",
          x: parseFloat(interactBar.dataset.dx || "0.5"),
          y: parseFloat(interactBar.dataset.dy || "0.85"),
        };
      }
      saveUiPrefs();
      interactBar.classList.remove("dragging");
      applyInteractPos();
    }
    // Clear on a timeout so the click that follows pointerup still sees
    // dragging=true and the buttons skip activation on a drag-release.
    setTimeout(() => { dragging = false; }, 0);
  };
  interactBar.addEventListener("pointerup", endDrag);
  interactBar.addEventListener("pointercancel", endDrag);
  interactBar.addEventListener("contextmenu", (ev) => ev.preventDefault());
  // Swallow a button tap that concludes a drag.
  interactBar.addEventListener("click", (ev) => {
    if (dragging) { ev.stopPropagation(); ev.preventDefault(); }
  }, true);

  // Expose so drag-end/apply can be reused; also apply the saved position
  // once at startup.
  window.__applyInteractPos = applyInteractPos;
})();

function flashInteractNote(text) {
  applyInteractPos();
  interactBar.hidden = false;
  document.getElementById("ia-cat").textContent = "";
  document.getElementById("ia-entity").textContent = text;
  clearTimeout(interactNoteTimer);
  interactNoteTimer = setTimeout(() => {
    if (!interact) interactBar.hidden = true;
  }, 1600);
}

function renderInteract() {
  if (!interact) {
    interactBar.hidden = true;
    return;
  }
  const current = interactCurrent();
  const cat = document.getElementById("ia-cat");
  const entity = document.getElementById("ia-entity");
  if (current) {
    cat.textContent = `${INTERACT_LABELS[current.category]} ${current.index + 1}/${current.count}`;
    entity.textContent = current.entity.label;
  } else {
    cat.textContent = INTERACT_LABELS[interact.category];
    entity.textContent = "nothing here";
  }
  applyInteractPos();
  interactBar.hidden = false;
}

/// Speak focus changes when speech is on (ephemeral UI — skip the queue
/// caps, mirror of the desktop TTS announce).
function announceInteractFocus() {
  const current = interactCurrent();
  if (!current || !speechPrefs.enabled || !speechAvailable()) return;
  const utterance = new SpeechSynthesisUtterance(
    `${current.entity.label}, ${INTERACT_LABELS[current.category]}`,
  );
  speechSynthesis.speak(utterance);
}

// Compact bar: one wrapping cycler per axis (lists are short; the pad's
// right stick still steps both directions), tap the label or ⏎ to act.
document.getElementById("interact-btn").addEventListener("click", toggleInteract);
document.getElementById("ia-close").addEventListener("click", exitInteract);
document.getElementById("ia-next").addEventListener("click", () => interactMove(1));
document.getElementById("ia-cat-next").addEventListener("click", () => interactCategoryMove(1));
document.getElementById("ia-go").addEventListener("click", interactActivate);
document.getElementById("ia-label").addEventListener("click", interactActivate);

function pollGamepads() {
  const pads = gamepadPads();
  if (!pads.length) {
    stopGamepadLoop();
    gpStickSector = null;
    gpWheel = null;
    gpWheelFired = false;
    gpAimRecenterNeeded = false;
    hideWheel();
    return;
  }
  for (const pad of pads) {
    const prev = gpPrev.get(pad.index) || [];
    pad.buttons.forEach((button, i) => {
      const pressed = !!(button && button.pressed);
      if (pressed && !prev[i]) handleGamepadButton(i);
    });
    gpPrev.set(pad.index, pad.buttons.map((b) => !!(b && b.pressed)));
  }

  const axes = pads[0].axes || [];
  const leftX = axes[0] || 0;
  const leftYUp = -(axes[1] || 0);
  const rightXRaw = axes[2] || 0;
  const rightYUp = -(axes[3] || 0);

  // Assign stick roles from movement_stick: the movement stick walks the
  // compass; the other ("aim" stick) aims the wheel, scrolls, and does
  // the interact cycling.
  const moveOnRight = wheelTuning.movement_stick === "right";
  const stickX = moveOnRight ? rightXRaw : leftX;
  const stickYUp = moveOnRight ? rightYUp : leftYUp;

  // Radial wheel: while a wheel-bound button is held, the aim stick aims
  // and a dwell commits a slice; releasing fires the committed leaf.
  // A sheet stealing the screen cancels instead of firing.
  const heldWheelKey =
    controllerPrefs.enabled && sheet.hidden ? gpHeldWheelKey() : null;
  // Which stick aims: a held wheel's `stick` override wins, else the
  // default aim stick (the non-movement one).
  const wheelName = heldWheelKey === "" ? "default" : heldWheelKey;
  const override = wheelName != null ? wheelStick[wheelName] : null;
  const aimOnRight = override === "right" ? true
    : override === "left" ? false
    : !moveOnRight;
  const aimX = aimOnRight ? rightXRaw : leftX;
  const aimYUp = aimOnRight ? rightYUp : leftYUp;
  const aimIsMoveStick = aimOnRight === moveOnRight;

  if (heldWheelKey === null && gpWheelFired) {
    // Fresh hold re-arms after a fired leaf; seed the movement hysteresis
    // so a still-deflected movement stick doesn't walk on release.
    gpWheelFired = false;
    gpStickSector = gpStickSectorFor(stickX, stickYUp, gpStickSector);
  }
  if (!gpWheel && heldWheelKey !== null && !gpWheelFired) {
    gpWheel = {
      key: heldWheelKey, path: [], aimed: null,
      candidate: null, candidateSince: 0, rearmUntilCenter: false,
      peakMagnitude: 0,
    };
    renderWheel();
  } else if (gpWheel && heldWheelKey === null) {
    // Release: fire the committed leaf (via host, with debounce).
    const { key, path, aimed } = gpWheel;
    gpWheel = null;
    hideWheel();
    if (sheet.hidden && aimed !== null) {
      const view = wheelView(key, path);
      const real = view ? WheelCore.leafRealAt(view, aimed) : null;
      if (real != null) wheelFire(key, [...path, real]);
    }
    gpStickSector = gpStickSectorFor(stickX, stickYUp, gpStickSector);
    gpAimRecenterNeeded = true;
  } else if (gpWheel) {
    wheelAim(aimX, aimYUp);
  }

  // Compass movement. Suppressed while a wheel owns the aim stick *and*
  // that aim stick is the movement stick, plus the fired-hold tail. When
  // the wheel aims with the other stick, walking stays live.
  const wheelUp = !!gpWheel || gpWheelFired;
  const wheelOwnsMove = wheelUp && aimIsMoveStick;
  if (!wheelOwnsMove) {
    const sector = gpStickSectorFor(stickX, stickYUp, gpStickSector);
    if (sector !== gpStickSector) {
      if (sector !== null && sheet.hidden && controllerPrefs.enabled) {
        sendCommand(GP_STICK_DIRS[sector]);
      }
      gpStickSector = sector;
    }
  }

  // Aim stick: interact-mode focus cycling, else story scroll. Silenced
  // while it aims an open wheel, and until it recenters after a wheel
  // closes (so leftover deflection can't scroll or cycle).
  if (gpAimRecenterNeeded && Math.hypot(aimX, aimYUp) < 0.35) {
    gpAimRecenterNeeded = false;
  }
  const aimOwnedByWheel = !!gpWheel || gpWheelFired || gpAimRecenterNeeded;
  if (aimOwnedByWheel) {
    // Keep the interact hysteresis synced so resuming doesn't fire a
    // stale cycle step.
    gpRightDir = gpFourWay(aimX, aimYUp, gpRightDir);
  } else if (interact && sheet.hidden) {
    const dir = gpFourWay(aimX, aimYUp, gpRightDir);
    if (dir !== gpRightDir) {
      if (dir === "up") interactCategoryMove(-1);
      else if (dir === "down") interactCategoryMove(1);
      else if (dir === "left") interactMove(-1);
      else if (dir === "right") interactMove(1);
      gpRightDir = dir;
    }
  } else if (Math.abs(aimYUp) > 0.25 && sheet.hidden) {
    // Stick up scrolls up (negative delta); quadratic speed curve.
    pane.scrollBy(0, -aimYUp * Math.abs(aimYUp) * 40);
  }
}

// Advance the dwell state machine one frame while the wheel is up. Thin
// adapter over WheelCore.wheelAimStep (the shared, node-tested machine):
// builds the view and tuning snapshot, injects the clock, then applies
// the outcome (repaint and/or a mid-hold edge/retract fire).
function wheelAim(x, yUp) {
  const view = wheelView(gpWheel.key, gpWheel.path);
  if (!view) return;
  const out = WheelCore.wheelAimStep(
    gpWheel, view, wheelTimingSnapshot(), x, yUp, performance.now(),
  );
  if (out.render) renderWheel();
  if (out.fire != null) wheelCloseAndFire(view, out.fire);
}

// [controller_tuning] snapshot in the units the machine consumes
// (magnitudes 0..1, dwells in ms).
function wheelTimingSnapshot() {
  return {
    deadzone: wheelDeadzone(),
    aimMs: wheelTuning.aim_dwell_ms || 0,
    navMs: wheelTuning.nav_dwell_ms || 0,
    fireMode: wheelTuning.fire_mode || "release",
    edgeThreshold: Math.min(1, Math.max(0, (wheelTuning.edge_threshold || 0) / 100)),
    retractDelta: Math.min(1, Math.max(0, (wheelTuning.retract_delta || 0) / 100)),
  };
}

// Close the wheel and fire the leaf at a display seat, if it is a real,
// non-folder seat (WheelCore.leafRealAt is the one shared guard). Used by
// the edge/retract mid-hold fires; the release branch uses leafRealAt
// directly (the wheel is already down there).
function wheelCloseAndFire(view, display) {
  const real = WheelCore.leafRealAt(view, display);
  if (real == null) return;
  const { key, path } = gpWheel;
  gpWheel = null;
  hideWheel();
  gpWheelFired = true;
  gpAimRecenterNeeded = true;
  wheelFire(key, [...path, real]);
}
// Send a wheel pick to the host (commands + <target_id> resolve there),
// honoring a fire debounce so a bounced button can't double-send.
let gpWheelLastFire = 0;
function wheelFire(key, path) {
  const debounce = Math.max(50, wheelTuning.fire_debounce_ms || 0);
  const now = performance.now();
  if (now - gpWheelLastFire < debounce) return;
  gpWheelLastFire = now;
  sendWheelPick(key, path);
}

// Dominant-axis four-way read of a stick with the movement hysteresis:
// one direction per deflection, re-armed by returning toward center.
let gpRightDir = null;
function gpFourWay(x, yUp, previous) {
  const magnitude = Math.hypot(x, yUp);
  if (magnitude < (previous !== null ? 0.35 : 0.6)) return null;
  if (Math.abs(x) > Math.abs(yUp)) return x > 0 ? "right" : "left";
  return yUp > 0 ? "up" : "down";
}

function sheetItemButtons() {
  return sheet.hidden ? [] : [...sheetItems.querySelectorAll("button.sheet-item")];
}

function gpSetSheetFocus(index) {
  const items = sheetItemButtons();
  if (!items.length) return;
  gpFocusIndex = ((index % items.length) + items.length) % items.length;
  items.forEach((item, i) => item.classList.toggle("gp-focus", i === gpFocusIndex));
  items[gpFocusIndex].scrollIntoView({ block: "nearest" });
}

function controllerSheetActive() {
  return !sheet.hidden && sheetTitle.textContent === "Controller";
}

function handleGamepadButton(index) {
  const name = GP_BUTTON_NAMES[index];
  if (!name) return;

  // The wheel owns the pad while it is up. Dwell drives navigation and
  // commit; South/East stay as optional accelerators — South fires the
  // slice under the stick now (or descends a folder / ascends via Back),
  // East backs up a level. Everything else is swallowed.
  if (gpWheel) {
    if (name === "south") {
      const { key, path } = gpWheel;
      const display = gpWheel.candidate != null ? gpWheel.candidate : gpWheel.aimed;
      if (display == null) return;
      const view = wheelView(key, path);
      if (!view) return;
      const real = view.realIndex[display];
      if (real == null || WheelCore.isBackSlice(view.slices[display])) {
        // Back seat — synthesized or an explicit back slice.
        if (gpWheel.path.length) {
          gpWheel.path.pop();
          gpWheel.aimed = null;
          gpWheel.candidate = null;
          gpWheel.rearmUntilCenter = true;
          renderWheel();
        }
        return;
      }
      const slice = view.slices[display];
      if ((slice.slices || []).length) {
        gpWheel.path.push(real);
        gpWheel.aimed = null;
        gpWheel.candidate = null;
        gpWheel.rearmUntilCenter = true;
        renderWheel();
      } else {
        gpWheel = null;
        gpWheelFired = true;
        gpAimRecenterNeeded = true;
        hideWheel();
        wheelFire(key, [...path, real]);
      }
    } else if (name === "east") {
      if (gpWheel.path.length) {
        gpWheel.path.pop();
        gpWheel.aimed = null;
        gpWheel.candidate = null;
        gpWheel.rearmUntilCenter = true;
        renderWheel();
      }
    }
    return;
  }

  // In the controller sheet, a physical press selects that button's row
  // for editing (capture) instead of dispatching.
  if (controllerSheetActive()) {
    gpEditButton = name;
    renderControllerSheet();
    return;
  }

  // Any open sheet (context menus, tap-to-target, settings): the d-pad
  // navigates, South taps the focused item, East closes.
  if (!sheet.hidden) {
    if (name === "dpad_up") return gpSetSheetFocus(gpFocusIndex <= 0 ? -1 : gpFocusIndex - 1);
    if (name === "dpad_down") return gpSetSheetFocus(gpFocusIndex + 1);
    if (name === "south") {
      const items = sheetItemButtons();
      const target = items[gpFocusIndex] || items[0];
      if (target) target.click();
      return;
    }
    if (name === "east") return closeSheet();
    return; // other buttons stay quiet while a sheet is open
  }

  // Interact mode: South selects (menu / walk); every other button —
  // d-pad, the remaining face buttons, and the whole shift bank — stays
  // on its binds, so West/North/East can carry <target_id> attack
  // macros. The right stick does the cycling (see pollGamepads); the
  // mode closes from its toggle, the ✕, or walking an exit.
  if (interact && !gpShiftHeld() && name === "south") {
    return interactActivate();
  }

  if (!controllerPrefs.enabled) return;
  if (gpShiftHeld()) {
    // Shift layer: strictly the shift bank — no fall-through.
    gpDispatchBind(controllerPrefs.shiftBinds[name]);
    return;
  }
  gpDispatchBind(controllerPrefs.binds[name]);
}

// True while any button bound to "shift" is held — read from live pad
// state so there is no press/release bookkeeping to desync.
function gpShiftHeld() {
  const shiftIndexes = Object.entries(GP_BUTTON_NAMES)
    .filter(([, name]) => controllerPrefs.binds[name] === GP_SHIFT)
    .map(([index]) => Number(index));
  if (!shiftIndexes.length) return false;
  return gamepadPads().some((pad) =>
    shiftIndexes.some((i) => !!(pad.buttons[i] && pad.buttons[i].pressed)),
  );
}

// Rumble the pad when a line arrives on a chosen stream (whispers and
// deaths by default) — the phone counterpart of the desktop event map.
let gpLastRumble = 0;
function gpRumbleLine(stream) {
  const rumble = controllerPrefs.rumble || {};
  if (rumble.enabled === false) return;
  const streams = rumble.streams || { whisper: true, death: true };
  if (!streams[stream]) return;
  const now = Date.now();
  if (now - gpLastRumble < 1500) return; // don't buzz continuously
  gpLastRumble = now;
  for (const pad of gamepadPads()) {
    pad.vibrationActuator
      ?.playEffect?.("dual-rumble", {
        duration: 250,
        strongMagnitude: 0.8,
        weakMagnitude: 0.4,
      })
      ?.catch?.(() => {});
  }
}

let gpSheetShiftLayer = false;

function openControllerSheet() {
  gpEditButton = null;
  gpSheetShiftLayer = false;
  openSheet("Controller");
  renderControllerSheet();
}

function renderControllerSheet() {
  sheetItems.replaceChildren();

  const pads = gamepadPads();
  if (pads.length) {
    sheetNote(`Connected: ${pads.map((p) => p.id).join(", ")}`, false);
  } else {
    sheetNote(
      "No controller detected — pair one over Bluetooth/USB and press a button.",
      false,
    );
  }

  const enabledBtn = document.createElement("button");
  enabledBtn.type = "button";
  enabledBtn.className = "sheet-item";
  enabledBtn.textContent = `${controllerPrefs.enabled ? "☑" : "☐"} Send bound commands`;
  enabledBtn.addEventListener("click", () => {
    controllerPrefs.enabled = !controllerPrefs.enabled;
    saveControllerPrefs();
    renderControllerSheet();
  });
  sheetItems.appendChild(enabledBtn);

  const layerBtn = document.createElement("button");
  layerBtn.type = "button";
  layerBtn.className = "sheet-item";
  layerBtn.textContent = gpSheetShiftLayer
    ? "Editing: shift layer (while shift held) — tap for base"
    : "Editing: base layer — tap for shift layer";
  layerBtn.addEventListener("click", () => {
    gpSheetShiftLayer = !gpSheetShiftLayer;
    gpEditButton = null;
    renderControllerSheet();
  });
  sheetItems.appendChild(layerBtn);

  // Rumble toggles (master + per-stream).
  const rumble =
    controllerPrefs.rumble ||
    (controllerPrefs.rumble = { enabled: true, streams: { whisper: true, death: true } });
  rumble.streams = rumble.streams || { whisper: true, death: true };
  const rumbleRow = (label, checked, onToggle) => {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "sheet-item";
    btn.textContent = `${checked ? "☑" : "☐"} ${label}`;
    btn.addEventListener("click", () => {
      onToggle();
      saveControllerPrefs();
      renderControllerSheet();
    });
    sheetItems.appendChild(btn);
  };
  rumbleRow("Rumble on events", rumble.enabled !== false, () => {
    rumble.enabled = rumble.enabled === false;
  });
  if (rumble.enabled !== false) {
    for (const stream of ["whisper", "death", "thoughts"]) {
      rumbleRow(`　rumble: ${stream}`, !!rumble.streams[stream], () => {
        rumble.streams[stream] = !rumble.streams[stream];
      });
    }
  }

  const activeBinds = gpSheetShiftLayer
    ? controllerPrefs.shiftBinds
    : controllerPrefs.binds;

  for (const name of GP_BUTTON_ORDER) {
    if (gpEditButton === name) {
      const row = document.createElement("div");
      row.className = "sheet-empty gp-edit-row";
      const input = document.createElement("input");
      input.type = "text";
      input.value = activeBinds[name] || "";
      input.placeholder = "command, shift, wheel, or action (e.g. right_panel)";
      const save = document.createElement("button");
      save.type = "button";
      save.textContent = "Save";
      save.addEventListener("click", () => {
        const value = input.value.trim();
        if (value) activeBinds[name] = value;
        else delete activeBinds[name];
        saveControllerPrefs();
        gpEditButton = null;
        renderControllerSheet();
      });
      const cancel = document.createElement("button");
      cancel.type = "button";
      cancel.textContent = "Cancel";
      cancel.addEventListener("click", () => {
        gpEditButton = null;
        renderControllerSheet();
      });
      row.append(`${name}: `, input, save, cancel);
      sheetItems.appendChild(row);
      queueMicrotask(() => input.focus());
      continue;
    }
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "sheet-item";
    const bind = activeBinds[name];
    btn.textContent = `${name} — ${bind || "unbound"}`;
    btn.addEventListener("click", () => {
      gpEditButton = name;
      renderControllerSheet();
    });
    sheetItems.appendChild(btn);
  }

  sheetNote(
    "Tap a row (or press the button on the pad) to edit. Bind a button " +
      'to "shift" to make it the hold-modifier for the shift layer. In ' +
      "menus the d-pad always navigates: up/down move, A taps, B closes.",
    false,
  );
  const named = Object.keys(wheels.named || {});
  sheetNote(
    'Bind "wheel" to hold-open the radial command wheel (defined on the ' +
      "host in keybinds.toml): aim with the left stick, release to fire; " +
      "A opens folders, B backs up. " +
      '"wheel:portals" is always available — its slices are the current ' +
      "room's noun exits (go gate, climb ladder)." +
      (named.length ? ` Named wheels: ${named.map((n) => `wheel:${n}`).join(", ")}.` : ""),
    false,
  );
  sheetNote(
    `UI actions are bindable too: ${Object.keys(GP_UI_ACTIONS).join(", ")}.`,
    false,
  );
}

function openSpeechSheet() {
  openSheet("Speech");
  if (!speechAvailable()) {
    sheetNote("Speech synthesis is not available in this browser.", true);
    return;
  }
  renderSpeechSheet();
}

function renderSpeechSheet() {
  sheetItems.replaceChildren();

  // Toggle rows rebuild the sheet in place instead of closing it
  // (sheetButton closes on pick, which would make toggling tedious).
  const toggleRow = (label, checked, onToggle) => {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "sheet-item";
    btn.textContent = `${checked ? "☑" : "☐"} ${label}`;
    btn.addEventListener("click", () => {
      onToggle();
      saveSpeechPrefs();
      renderSpeechSheet();
    });
    sheetItems.appendChild(btn);
  };

  toggleRow("Speak incoming lines", speechPrefs.enabled, () => {
    speechPrefs.enabled = !speechPrefs.enabled;
    if (!speechPrefs.enabled) stopSpeaking();
  });

  // Rate slider, styled like the appearance sheet's opacity rows.
  const rateRow = document.createElement("div");
  rateRow.className = "alpha-row";
  const rateLabel = document.createElement("label");
  rateLabel.textContent = "Rate";
  const rateSlider = document.createElement("input");
  rateSlider.type = "range";
  rateSlider.min = "0.5";
  rateSlider.max = "3";
  rateSlider.step = "0.1";
  rateSlider.value = String(speechPrefs.rate || 1.0);
  const rateValue = document.createElement("span");
  rateValue.className = "alpha-value";
  rateValue.textContent = `${rateSlider.value}x`;
  rateSlider.addEventListener("input", () => {
    speechPrefs.rate = Number(rateSlider.value);
    rateValue.textContent = `${rateSlider.value}x`;
    saveSpeechPrefs();
  });
  rateRow.append(rateLabel, rateSlider, rateValue);
  sheetItems.appendChild(rateRow);

  // Voice picker. Voice lists load async on some platforms (iOS); the
  // voiceschanged hook below re-renders when they arrive.
  const voices = speechSynthesis.getVoices();
  if (voices.length) {
    const voiceRow = document.createElement("div");
    voiceRow.className = "alpha-row";
    const voiceLabel = document.createElement("label");
    voiceLabel.textContent = "Voice";
    const voiceSelect = document.createElement("select");
    const defaultOption = document.createElement("option");
    defaultOption.value = "";
    defaultOption.textContent = "(device default)";
    voiceSelect.appendChild(defaultOption);
    for (const voice of voices) {
      const option = document.createElement("option");
      option.value = voice.name;
      option.textContent = voice.name;
      voiceSelect.appendChild(option);
    }
    voiceSelect.value = speechVoiceByName(speechPrefs.voice) ? speechPrefs.voice : "";
    voiceSelect.addEventListener("change", () => {
      speechPrefs.voice = voiceSelect.value;
      saveSpeechPrefs();
    });
    voiceRow.append(voiceLabel, voiceSelect);
    sheetItems.appendChild(voiceRow);
  }

  const streamsHeading = document.createElement("div");
  streamsHeading.className = "sheet-empty";
  streamsHeading.textContent = "Streams to speak:";
  sheetItems.appendChild(streamsHeading);
  for (const stream of speechStreamChoices()) {
    toggleRow(STREAM_LABELS[stream] || stream, !!speechPrefs.streams[stream], () => {
      speechPrefs.streams[stream] = !speechPrefs.streams[stream];
    });
  }

  const testBtn = document.createElement("button");
  testBtn.type = "button";
  testBtn.className = "sheet-item";
  testBtn.textContent = "Test voice";
  testBtn.addEventListener("click", () => {
    // Speaking from a tap also satisfies mobile user-gesture unlock.
    const utterance = new SpeechSynthesisUtterance(
      "A giant rat scampers out of the shadows.",
    );
    utterance.rate = Math.min(Math.max(speechPrefs.rate || 1.0, 0.5), 3.0);
    const voice = speechVoiceByName(speechPrefs.voice);
    if (voice) utterance.voice = voice;
    speechSynthesis.speak(utterance);
  });
  sheetItems.appendChild(testBtn);

  const stopBtn = document.createElement("button");
  stopBtn.type = "button";
  stopBtn.className = "sheet-item";
  stopBtn.textContent = "Stop speaking";
  stopBtn.addEventListener("click", stopSpeaking);
  sheetItems.appendChild(stopBtn);
}

if (speechAvailable()) {
  // iOS/Android load voice lists asynchronously; refresh the sheet if it's
  // showing when they land.
  speechSynthesis.addEventListener?.("voiceschanged", () => {
    if (!sheet.hidden && sheetTitle.textContent === "Speech") renderSpeechSheet();
  });
}

// ---- Settings sheet + config editor ---------------------------------------
// Config files live server-side (on Android, in the app's private storage
// where no file manager reaches) — the editor is how highlights and colors
// get onto the device: paste or import a desktop file, Save validates the
// TOML server-side and hot-reloads.

// Raw TOML editors: the import/export path for desktop configs and the
// power-user escape hatch — the friendly editors are the primary UI.
const EDITOR_FILES = [
  { id: "highlights", label: "Advanced: highlights file (this profile)", filename: "highlights.toml" },
  { id: "highlights-global", label: "Advanced: highlights file (global)", filename: "highlights.toml" },
  { id: "colors", label: "Advanced: colors file (this profile)", filename: "colors.toml" },
  { id: "colors-global", label: "Advanced: colors file (global)", filename: "colors.toml" },
];

const editorOverlay = document.getElementById("editor-overlay");
const editorTitle = document.getElementById("editor-title");
const editorText = document.getElementById("editor-text");
const editorStatus = document.getElementById("editor-status");
let editorFile = null; // active EDITOR_FILES entry
let configRequestCounter = 0;
let pendingConfigRequest = null;

// ---- Appearance (client-side prefs) ----------------------------------------
// Theme presets are CSS-variable override sets; chrome toggles are body
// classes. Everything persists per device, except the theme choice,
// which is also a roaming pref (web.theme): a server-side value wins on
// connect and picks push back to the character profile.

const UI_PREFS_KEY = "vellum-ui-prefs";
let uiPrefs = { theme: "dark", hide: {} };
try {
  const stored = JSON.parse(localStorage.getItem(UI_PREFS_KEY) || "{}");
  if (stored && typeof stored === "object") {
    // Spread so per-device extras (opacity, interactPos, …) survive a
    // reload; only theme/hide have hard defaults.
    uiPrefs = { ...stored, theme: stored.theme || "dark", hide: stored.hide || {} };
  }
} catch { /* defaults */ }
// The bottom macro rail duplicates the tray + floating buttons on very
// limited vertical space: hidden unless the user explicitly enabled it.
if (!("macrorail" in uiPrefs.hide)) uiPrefs.hide.macrorail = true;

const THEMES = {
  dark: { label: "Vellum dark", vars: {} },
  black: {
    label: "Pure black (OLED)",
    vars: {
      "--bg": "#000000", "--bg-panel": "#0b0b0e", "--border": "#1e2128",
      "--panel-rgb": "11, 11, 14",
    },
  },
  contrast: {
    label: "High contrast",
    vars: {
      "--bg": "#000000", "--bg-panel": "#101014", "--fg": "#ffffff",
      "--fg-dim": "#c0c6cf", "--border": "#4a5060",
      "--panel-rgb": "16, 16, 20",
    },
  },
  light: {
    label: "Parchment",
    vars: {
      "--bg": "#f2efe9", "--bg-panel": "#e6e1d8", "--fg": "#2a2620",
      "--fg-dim": "#6b655c", "--border": "#c9c2b4", "--st": "#8a6d1f",
      "--panel-rgb": "230, 225, 216",
    },
  },
};

// Adjustable panel opacities (percent), applied as CSS alpha variables.
const OPACITY_SETTINGS = [
  ["float", "Floating buttons", "--float-alpha", 82],
  ["drawer", "Side drawers", "--drawer-alpha", 93],
  ["sheet", "Bottom menus", "--sheet-alpha", 100],
  ["interact", "Interact bar", "--interact-alpha", 93],
];

const CHROME_TOGGLES = [
  ["macrorail", "Macro bar (bottom)"],
  ["compass", "Compass"],
  ["interact", "Interact button"],
  ["vitals", "Vitals bars"],
  ["hands", "Hands"],
  ["rt", "RT label"],
  ["fx", "Effect pills"],
  ["chips", "Stream chips"],
];

function saveUiPrefs() {
  try {
    localStorage.setItem(UI_PREFS_KEY, JSON.stringify(uiPrefs));
  } catch { /* private mode */ }
}

function applyUiPrefs() {
  const root = document.documentElement;
  for (const theme of Object.values(THEMES)) {
    for (const key of Object.keys(theme.vars)) root.style.removeProperty(key);
  }
  const active = THEMES[uiPrefs.theme] || THEMES.dark;
  for (const [key, value] of Object.entries(active.vars)) {
    root.style.setProperty(key, value);
  }
  for (const [key] of CHROME_TOGGLES) {
    document.body.classList.toggle(`hide-${key}`, !!uiPrefs.hide[key]);
  }
  const opacity = uiPrefs.opacity || {};
  for (const [key, , cssVar, dflt] of OPACITY_SETTINGS) {
    const pct = Number.isFinite(opacity[key]) ? opacity[key] : dflt;
    root.style.setProperty(cssVar, String(Math.min(100, Math.max(20, pct)) / 100));
  }
  applyInteractPos();
}
applyUiPrefs();

function openAppearanceSheet() {
  openSheet("Appearance");

  const themeRow = document.createElement("div");
  themeRow.className = "theme-row";
  const themeButtons = new Map();
  for (const [key, theme] of Object.entries(THEMES)) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "theme-btn";
    btn.textContent = theme.label;
    btn.addEventListener("click", () => {
      uiPrefs.theme = key;
      saveUiPrefs();
      applyUiPrefs();
      refreshThemeButtons();
      roamPut("web.theme", key);
    });
    themeButtons.set(key, btn);
    themeRow.appendChild(btn);
  }
  const refreshThemeButtons = () => {
    for (const [key, btn] of themeButtons) {
      btn.classList.toggle("theme-active", key === (uiPrefs.theme || "dark"));
    }
  };
  refreshThemeButtons();
  sheetItems.appendChild(themeRow);

  for (const [key, label, , dflt] of OPACITY_SETTINGS) {
    const row = document.createElement("div");
    row.className = "alpha-row";
    const lab = document.createElement("label");
    lab.textContent = label;
    const slider = document.createElement("input");
    slider.type = "range";
    slider.min = "20";
    slider.max = "100";
    slider.step = "5";
    const current = (uiPrefs.opacity || {})[key];
    slider.value = String(Number.isFinite(current) ? current : dflt);
    const value = document.createElement("span");
    value.className = "alpha-value";
    const refresh = () => { value.textContent = `${slider.value}%`; };
    slider.addEventListener("input", () => {
      uiPrefs.opacity = uiPrefs.opacity || {};
      uiPrefs.opacity[key] = Number(slider.value);
      saveUiPrefs();
      applyUiPrefs();
      refresh();
    });
    refresh();
    row.append(lab, slider, value);
    sheetItems.appendChild(row);
  }

  for (const [key, label] of CHROME_TOGGLES) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "sheet-item";
    const refresh = () => {
      btn.textContent = `${label}: ${uiPrefs.hide[key] ? "hidden" : "shown"}`;
    };
    btn.addEventListener("click", () => {
      uiPrefs.hide[key] = !uiPrefs.hide[key];
      saveUiPrefs();
      applyUiPrefs();
      refresh();
    });
    refresh();
    sheetItems.appendChild(btn);
  }
  sheetNote("Changes apply live — tap anywhere else to close", true);
}

function openSettingsSheet() {
  openSheet("Settings");
  sheetButton("Appearance", openAppearanceSheet);
  sheetButton("Speech (read lines aloud)", openSpeechSheet);
  sheetButton("Controller (gamepad)", openControllerSheet);
  sheetButton(
    soundMuted ? "Sound alerts: off — tap to enable" : "Sound alerts: on — tap to mute",
    () => {
      soundMuted = !soundMuted;
      try {
        localStorage.setItem(SOUND_MUTE_KEY, soundMuted ? "1" : "");
      } catch { /* private mode */ }
    },
  );
  // Login music only exists where a login screen does (session_control).
  if (session.session_control) {
    sheetButton(
      musicOff ? "Login music: off — tap to enable" : "Login music: on — tap to disable",
      () => setMusicOff(!musicOff),
    );
  }
  sheetButton("Client settings (saved on host)", openClientSettings);
  sheetButton("Streams (saved on host)", openStreamsPanel);
  sheetButton("Highlight rules (this profile)", () => openHighlightList("profile"));
  sheetButton("Highlight rules (global)", () => openHighlightList("global"));
  sheetButton("Colors (this profile)", () => openColorsEditor("profile"));
  sheetButton("Colors (global)", () => openColorsEditor("global"));
  // Raw-TOML editing is an escape hatch, not a front door — editing TOML on
  // a soft keyboard is a footgun and the structured editors above cover the
  // same ground. Tuck the four file buttons behind one disclosure so they're
  // reachable for import/export power users without crowding the main sheet.
  sheetButton("Advanced: edit config files…", openAdvancedConfigSheet);
  // Viewing a desktop server from inside the app shell: the way home.
  // (The shell swaps the WebView back to its embedded login page.)
  if (inShell && location.hostname !== "127.0.0.1") {
    sheetButton("Leave this server (app login)", () => {
      location.href = "vellum://local";
    });
  }
}

// Advanced raw-TOML config editing, one step removed from the main sheet.
// Import/export of desktop configs lives here; the structured Highlight and
// Colors editors on the main sheet are the primary path.
function openAdvancedConfigSheet() {
  openSheet("Advanced: config files");
  sheetNote("Raw TOML — the structured editors are easier and safer.", false);
  for (const file of EDITOR_FILES) {
    sheetButton(file.label.replace(/^Advanced:\s*/, ""), () => openConfigEditor(file));
  }
  sheetButton("‹ Back to settings", openSettingsSheet);
}

document.getElementById("settings-btn").addEventListener("click", openSettingsSheet);
// The login screen covers the top bar; it gets its own settings entry so
// highlights/appearance are reachable before logging in.
document.getElementById("session-settings").addEventListener("click", openSettingsSheet);

// ---- Highlight rule editor -------------------------------------------------
// Structured editing of one scope's highlights file: list → tap → form.
// The form covers the common fields; anything else a desktop hand-edit set
// (redirects, replace, squelch, ...) is preserved by merging into the
// fetched rule rather than rebuilding it.

const hlOverlay = document.getElementById("hl-overlay");
const hlTitle = document.getElementById("hl-title");
const hlListView = document.getElementById("hl-list-view");
const hlListEl = document.getElementById("hl-list");
const hlForm = document.getElementById("hl-form");
const hlStatus = document.getElementById("hl-status");
let hlScope = null;
let hlRules = {};
let hlSounds = [];
let hlEditingName = null; // null = list view, "" = new rule
let hlRequestCounter = 0;
let hlPendingRequest = null;
let hlAwaitingSave = false;

function hlStatusMsg(text, isError) {
  hlStatus.textContent = text;
  hlStatus.classList.toggle("editor-error", !!isError);
  hlStatus.hidden = !text;
}

function openHighlightList(scope) {
  hlScope = scope;
  hlTitle.textContent =
    scope === "global" ? "Highlight rules (global)" : "Highlight rules (this profile)";
  hlRules = {};
  hlEditingName = null;
  hlOverlay.hidden = false;
  showHlList();
  hlListEl.replaceChildren(Object.assign(document.createElement("p"), {
    className: "hl-empty", textContent: "Loading…",
  }));
  hlPendingRequest = ++hlRequestCounter;
  sendJson("highlights_get", { request_id: hlPendingRequest, scope });
}

function showHlList() {
  hlForm.hidden = true;
  hlListView.hidden = false;
  renderHlList();
}

function renderHlList() {
  hlListEl.replaceChildren();
  const names = Object.keys(hlRules).sort((a, b) => a.localeCompare(b));
  if (!names.length) {
    hlListEl.appendChild(Object.assign(document.createElement("p"), {
      className: "hl-empty",
      textContent: "No rules yet — tap New rule, or import a file from Settings.",
    }));
    return;
  }
  for (const name of names) {
    const rule = hlRules[name];
    const row = document.createElement("button");
    row.type = "button";
    row.className = "hl-row";
    const swatch = document.createElement("span");
    swatch.className = "hl-swatch";
    if (rule.fg) swatch.style.background = rule.fg;
    const label = document.createElement("span");
    label.className = "hl-row-name";
    label.textContent = name;
    const pattern = document.createElement("span");
    pattern.className = "hl-row-pattern";
    pattern.textContent = rule.pattern || "";
    row.append(swatch, label, pattern);
    row.addEventListener("click", () => openHlForm(name));
    hlListEl.appendChild(row);
  }
}

// Color state: null = "no color set". The native pickers can't represent
// empty, so the None buttons carry that state and the preview reflects it.
let hlColors = { fg: null, bg: null };

function openHlForm(name) {
  hlEditingName = name;
  const rule = name ? hlRules[name] || {} : {};
  document.getElementById("hl-name").value = name;
  document.getElementById("hl-pattern").value = rule.pattern || "";
  // New rules start with a selected text color (the 99% case); existing
  // rules reflect exactly what they have. Picker widgets are reset per
  // form so the previous rule's choices don't leak into this one.
  if (name) {
    hlColors = { fg: rule.fg || null, bg: rule.bg || null };
  } else {
    hlColors = { fg: "#ffff00", bg: null };
  }
  document.getElementById("hl-fg-pick").value =
    hlColors.fg && /^#[0-9a-f]{6}$/i.test(hlColors.fg) ? hlColors.fg : "#ffff00";
  document.getElementById("hl-bg-pick").value =
    hlColors.bg && /^#[0-9a-f]{6}$/i.test(hlColors.bg) ? hlColors.bg : "#333333";
  document.getElementById("hl-bold").checked = !!rule.bold;
  document.getElementById("hl-line").checked = !!rule.color_entire_line;
  document.getElementById("hl-fast-parse").checked = !!rule.fast_parse;
  document.getElementById("hl-squelch").checked = !!rule.squelch;
  document.getElementById("hl-silent-prompt").checked = !!rule.silent_prompt;
  document.getElementById("hl-category").value = rule.category || "";
  document.getElementById("hl-sound-volume").value =
    rule.sound_volume != null ? rule.sound_volume : "";
  document.getElementById("hl-rumble").value = rule.rumble || "";
  document.getElementById("hl-replace").value = rule.replace || "";
  document.getElementById("hl-stream").value = rule.stream || "";
  document.getElementById("hl-window").value = rule.window || "";
  document.getElementById("hl-set-status").value = rule.set_status || "";
  document.getElementById("hl-status-duration").value =
    rule.status_duration != null ? rule.status_duration : "";
  document.getElementById("hl-clear-status").value = rule.clear_status || "";
  // Redirect: "off" when there's no target; else copy/only from the mode.
  document.getElementById("hl-redirect-to").value = rule.redirect_to || "";
  document.getElementById("hl-redirect-mode").value = !rule.redirect_to
    ? "off"
    : rule.redirect_mode === "redirect_copy"
      ? "copy"
      : "only";

  // Sound dropdown: server-listed files, plus the rule's current value if
  // it references something not in the folder (keeps it selectable).
  const soundSel = document.getElementById("hl-sound");
  soundSel.replaceChildren(new Option("None", ""));
  const sounds = [...hlSounds];
  if (rule.sound && !sounds.includes(rule.sound)) sounds.unshift(rule.sound);
  for (const file of sounds) soundSel.appendChild(new Option(file, file));
  soundSel.value = rule.sound || "";

  document.getElementById("hl-delete").hidden = !name;
  hlStatusMsg("", false);
  updateHlPreview();
  hlListView.hidden = true;
  hlForm.hidden = false;
}

function updateHlPreview() {
  const preview = document.getElementById("hl-preview");
  const text = document.getElementById("hl-preview-text");
  const pattern = document.getElementById("hl-pattern").value.trim();
  // Show the pattern itself when it reads like plain text; regex syntax
  // gets a generic sample.
  text.textContent =
    pattern && !/[\\^$.|?*+()\[\]{}]/.test(pattern) ? pattern : "Sample game text";
  const wholeLine = document.getElementById("hl-line").checked;
  const target = wholeLine ? preview : text;
  const other = wholeLine ? text : preview;
  other.style.color = "";
  other.style.background = "";
  target.style.color = hlColors.fg || "";
  target.style.background = hlColors.bg || "";
  preview.style.fontWeight = text.style.fontWeight =
    document.getElementById("hl-bold").checked ? "bold" : "";
  document.getElementById("hl-fg-clear").classList.toggle("hl-none-active", !hlColors.fg);
  document.getElementById("hl-bg-clear").classList.toggle("hl-none-active", !hlColors.bg);
  document.getElementById("hl-fg-pick").classList.toggle("hl-inactive", !hlColors.fg);
  document.getElementById("hl-bg-pick").classList.toggle("hl-inactive", !hlColors.bg);
}

// Tapping a picker adopts the color it already shows (choosing the shown
// color in the dialog fires no input event — the value didn't change);
// dragging in the dialog then updates live via input.
for (const [pickId, key] of [["hl-fg-pick", "fg"], ["hl-bg-pick", "bg"]]) {
  const pick = document.getElementById(pickId);
  pick.addEventListener("click", () => {
    hlColors[key] = pick.value;
    updateHlPreview();
  });
  pick.addEventListener("input", () => {
    hlColors[key] = pick.value;
    updateHlPreview();
  });
}
document.getElementById("hl-fg-clear").addEventListener("click", () => {
  hlColors.fg = null;
  updateHlPreview();
});
document.getElementById("hl-bg-clear").addEventListener("click", () => {
  hlColors.bg = null;
  updateHlPreview();
});
document.getElementById("hl-pattern").addEventListener("input", updateHlPreview);
document.getElementById("hl-bold").addEventListener("change", updateHlPreview);
document.getElementById("hl-line").addEventListener("change", updateHlPreview);

function handleHighlightsReply(d) {
  if (d.request_id !== hlPendingRequest) return;
  if (d.error) {
    // Show the error wherever the user is (form save or list load).
    if (!hlForm.hidden) {
      hlStatusMsg(d.error, true);
    } else {
      hlListEl.replaceChildren(Object.assign(document.createElement("p"), {
        className: "hl-empty editor-error", textContent: d.error,
      }));
    }
    hlAwaitingSave = false;
    return;
  }
  hlRules = d.rules || {};
  hlSounds = d.sounds || [];
  // The server ships the canonical highlight-field catalog (config::
  // HIGHLIGHT_FIELDS). If it ever lists a field this form has no control for,
  // that field silently can't be edited on the phone — surface it in the dev
  // console so the drift is visible rather than invisible. Element ids follow
  // the hl-<field> convention with a few historical aliases.
  if (Array.isArray(d.fields)) {
    const idFor = { fg: "hl-fg-pick", bg: "hl-bg-pick", color_entire_line: "hl-line" };
    const missing = d.fields.filter((f) => {
      const id = idFor[f] || `hl-${f.replace(/_/g, "-")}`;
      return !document.getElementById(id);
    });
    if (missing.length) {
      console.warn("Highlight form is missing controls for server fields:", missing);
    }
  }
  if (hlAwaitingSave) {
    hlAwaitingSave = false;
    showHlList();
  } else {
    renderHlList();
  }
}

hlForm.addEventListener("submit", (ev) => {
  ev.preventDefault();
  const name = document.getElementById("hl-name").value.trim();
  const pattern = document.getElementById("hl-pattern").value;
  if (!name || !pattern.trim()) {
    hlStatusMsg("Name and pattern are required.", true);
    return;
  }
  // Merge the form over the existing rule so any future field the form
  // doesn't yet cover survives the round trip. The form now edits the full
  // HighlightPattern, so each control writes its field (and clears it when
  // empty rather than leaving a stale value from `base`).
  const base = (hlEditingName && hlRules[hlEditingName]) || {};
  const rule = { ...base, pattern };
  // Small helpers: set from a text input or delete the key when blank.
  const setText = (key, id) => {
    const v = document.getElementById(id).value.trim();
    if (v) rule[key] = v; else delete rule[key];
  };
  const sound = document.getElementById("hl-sound").value;
  if (hlColors.fg) rule.fg = hlColors.fg; else delete rule.fg;
  if (hlColors.bg) rule.bg = hlColors.bg; else delete rule.bg;
  if (sound) rule.sound = sound; else delete rule.sound;
  rule.bold = document.getElementById("hl-bold").checked;
  rule.color_entire_line = document.getElementById("hl-line").checked;
  rule.fast_parse = document.getElementById("hl-fast-parse").checked;
  rule.squelch = document.getElementById("hl-squelch").checked;
  rule.silent_prompt = document.getElementById("hl-silent-prompt").checked;
  setText("category", "hl-category");
  setText("rumble", "hl-rumble");
  setText("replace", "hl-replace");
  setText("stream", "hl-stream");
  setText("window", "hl-window");
  setText("set_status", "hl-set-status");
  setText("clear_status", "hl-clear-status");

  // Sound volume: a number in [0, 1], or cleared when blank/invalid.
  const volRaw = document.getElementById("hl-sound-volume").value.trim();
  const vol = parseFloat(volRaw);
  if (volRaw !== "" && !Number.isNaN(vol)) {
    rule.sound_volume = Math.min(1, Math.max(0, vol));
  } else {
    delete rule.sound_volume;
  }

  // Status duration: seconds until the set status auto-clears; no upper
  // clamp, cleared when blank/invalid.
  const durRaw = document.getElementById("hl-status-duration").value.trim();
  const dur = parseFloat(durRaw);
  if (durRaw !== "" && !Number.isNaN(dur) && dur >= 0) {
    rule.status_duration = dur;
  } else {
    delete rule.status_duration;
  }

  // Redirect: the mode select drives both fields. "Off" clears the redirect
  // entirely; otherwise redirect_to is required and redirect_mode is set.
  const redirectTo = document.getElementById("hl-redirect-to").value.trim();
  const redirectMode = document.getElementById("hl-redirect-mode").value;
  if (redirectMode === "off" || !redirectTo) {
    delete rule.redirect_to;
    delete rule.redirect_mode;
  } else {
    rule.redirect_to = redirectTo;
    rule.redirect_mode =
      redirectMode === "copy" ? "redirect_copy" : "redirect_only";
  }

  hlStatusMsg("Saving…", false);
  hlAwaitingSave = true;
  hlPendingRequest = ++hlRequestCounter;
  sendJson("highlight_put", {
    request_id: hlPendingRequest,
    scope: hlScope,
    name,
    rule,
  });
  // Renaming: the old rule is a separate entry — remove it after the new
  // one saves. Server replies with the updated map either way.
  if (hlEditingName && hlEditingName !== name) {
    sendJson("highlight_delete", {
      request_id: hlPendingRequest,
      scope: hlScope,
      name: hlEditingName,
    });
  }
});

document.getElementById("hl-delete").addEventListener("click", () => {
  if (!hlEditingName) return;
  hlStatusMsg("Deleting…", false);
  hlAwaitingSave = true;
  hlPendingRequest = ++hlRequestCounter;
  sendJson("highlight_delete", {
    request_id: hlPendingRequest,
    scope: hlScope,
    name: hlEditingName,
  });
});

document.getElementById("hl-back").addEventListener("click", showHlList);
document.getElementById("hl-new").addEventListener("click", () => openHlForm(""));
document.getElementById("hl-close").addEventListener("click", () => {
  hlOverlay.hidden = true;
  hlScope = null;
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

// ---- Login music ------------------------------------------------------------
// Nostalgia: the classic Wizard FE theme plays once per page load when the
// game connection is established — the mobile analog of the TUI's
// [sound] startup_music. The bar's Stop silences this play; "Don't play
// again" (and the settings-sheet toggle) persists. Tapping Connect counts
// as the user interaction browsers want before audio, so plain browsers
// usually allow it too; if a strict autoplay policy still rejects play(),
// nothing appears.

// Login music is OPT-IN on the phone: a phone auto-playing the Wizard FE
// theme on every connect is a surprise-audio liability (public spaces,
// battery). Stored as an explicit opt-IN flag; absent = off.
const MUSIC_ON_KEY = "vellum-login-music-on";
let musicOff = true;
try {
  musicOff = !localStorage.getItem(MUSIC_ON_KEY);
} catch { /* default off */ }

const musicBar = document.getElementById("music-bar");
let loginMusic = null; // Audio element while playing
let musicPlayed = false; // once per page load

function stopLoginMusic() {
  if (loginMusic) {
    loginMusic.pause();
    loginMusic = null;
  }
  musicBar.hidden = true;
}

function setMusicOff(off) {
  musicOff = off;
  try {
    if (off) {
      localStorage.removeItem(MUSIC_ON_KEY);
    } else {
      localStorage.setItem(MUSIC_ON_KEY, "1");
    }
  } catch { /* private mode */ }
  if (off) stopLoginMusic();
}

function maybePlayLoginMusic() {
  if (musicPlayed || musicOff || soundMuted) return;
  musicPlayed = true;
  const audio = new Audio(`/sounds/wizard_music?token=${encodeURIComponent(pairingToken)}`);
  audio.addEventListener("ended", stopLoginMusic);
  audio.play().then(() => {
    loginMusic = audio;
    musicBar.hidden = false;
  }).catch((e) => {
    console.debug("login music blocked or missing", e);
  });
}

document.getElementById("music-stop").addEventListener("click", stopLoginMusic);
document.getElementById("music-never").addEventListener("click", () => setMusicOff(true));

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
  gpFocusIndex = -1; // fresh sheet, fresh d-pad focus
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

// A pad connected before this script ran fires no gamepadconnected event;
// pick it up if the browser already lists one.
if (gamepadPads().length) startGamepadLoop();

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
  if (ev.target.closest("#interact-bar")) return; // Go opens the entity menu
  if (ev.target.closest("#interact-btn")) return; // toggling shouldn't dismiss
  if (ev.target.closest("#textsize-btn")) return; // opens the size stepper
  if (ev.target.closest("#settings-btn")) return; // opens the settings sheet
  if (ev.target.closest("#session-settings")) return; // login-screen settings
  if (ev.target.closest("#logout-btn")) return; // opens the disconnect confirm
  if (ev.target.closest("#session-banner")) return; // opens stop-reconnect
  if (ev.target.closest(".theme-row")) return; // appearance picks retarget
  if (ev.target.closest(".float-btn")) return;
  if (ev.target.closest(".drawer")) return; // tray buttons open option sheets
  if (ev.target.closest(".drawer-handle")) return;
  closeSheet();
});

pane.addEventListener("click", (ev) => {
  const span = ev.target.closest("span.link");
  if (!span || !state.ws || state.ws.readyState !== WebSocket.OPEN) return;
  // Web links (<a href> in game text) open on THIS device, never the host.
  if (span.dataset.existId === "_url_") {
    const url = span.dataset.noun || "";
    if (/^https?:\/\//.test(url)) window.open(url, "_blank", "noopener");
    return;
  }
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
//
// Type-in (`insert`) buttons never round-trip: their text arrives with
// the definition and a tap types it into the command input, so word
// buttons compose phrases ("go" + "second" + "door"). A trailing \r in
// the text means "then press Send" — the whole composed line goes.

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

// Type a macro's text into the command input. Words join with a space
// (authored text is trimmed everywhere, so "go" + "second" must not
// become "gosecond"). Deliberately no focus() — popping the soft
// keyboard would defeat the button workflow.
function insertMacroText(text) {
  if (!text) return;
  const send = text.endsWith("\r");
  if (send) text = text.slice(0, -1);
  if (text && cmdInput.value && !/\s$/.test(cmdInput.value)) {
    cmdInput.value += " ";
  }
  cmdInput.value += text;
  if (send) submitInput();
}

// A button or menu option, post-confirm: type-in entries stay local,
// everything else round-trips by id.
function fireMacro(entry) {
  if (entry.insert) insertMacroText(entry.command || "");
  else sendMacro(entry.id);
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
        if (opt.confirm) confirmSheet(opt.label, () => fireMacro(opt));
        else fireMacro(opt);
      });
    }
    return;
  }
  if (btn.confirm) confirmSheet(btn.label, () => fireMacro(btn));
  else fireMacro(btn);
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
  // Keep the left drawer tray in sync with definition changes.
  if (typeof renderTray === "function" &&
      document.getElementById("drawer-left").classList.contains("open")) {
    renderTray();
  }
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

// The editor never exposes the \r storage convention: tap behavior is a
// three-way picker, and the trailing \r ("type, then send") is appended
// and stripped here on save/load.
const TAP_MODES = [
  ["send", "Send the command"],
  ["insert", "Type into input"],
  ["insert-send", "Type, then send"],
];

function tapModeOf(entry) {
  if (!entry || !entry.insert) return "send";
  return (entry.command || "").endsWith("\r") ? "insert-send" : "insert";
}

function tapSelect(mode) {
  const select = document.createElement("select");
  for (const [value, label] of TAP_MODES) select.append(new Option(label, value));
  select.value = mode;
  return select;
}

function encodeTapMode(command, mode) {
  return mode === "insert-send" && command ? command + "\r" : command;
}

function stripTapEnter(command) {
  return (command || "").replace(/\r$/, "");
}

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
  cmdInputEl.value = existing ? stripTapEnter(existing.btn.command) : "";
  const cmdWrap = document.createElement("label");
  cmdWrap.append("Command (leave empty for a menu button)", cmdInputEl);

  // Tap behavior: send now, type into the input (composable word
  // buttons), or type and submit the whole composed line.
  const tapModeIn = tapSelect(tapModeOf(existing?.btn));
  const tapWrap = document.createElement("label");
  tapWrap.append("On tap", tapModeIn);

  // Menu options: with any options, tapping the button opens a picker
  // sheet instead of firing a command — "a button that is a category".
  const optionsWrap = document.createElement("div");
  optionsWrap.className = "option-rows";
  const optionRows = [];
  function addOptionRow(label = "", command = "", confirmed = false, tapMode = "send") {
    const row = document.createElement("div");
    row.className = "option-row";
    const main = document.createElement("div");
    main.className = "option-row-main";
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
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "option-remove";
    remove.textContent = "✕";
    remove.addEventListener("click", () => {
      row.remove();
      optionRows.splice(optionRows.indexOf(entry), 1);
    });
    main.append(labelIn, cmdIn, remove);
    const extras = document.createElement("div");
    extras.className = "option-row-extras";
    const tapIn = tapSelect(tapMode);
    const confirmIn = document.createElement("input");
    confirmIn.type = "checkbox";
    confirmIn.checked = confirmed;
    const confirmLabel = document.createElement("label");
    confirmLabel.append(confirmIn, "confirm");
    extras.append(tapIn, confirmLabel);
    const entry = { labelIn, cmdIn, confirmIn, tapIn };
    optionRows.push(entry);
    row.append(main, extras);
    optionsWrap.appendChild(row);
  }
  for (const opt of existing?.btn.options || []) {
    addOptionRow(opt.label, stripTapEnter(opt.command), opt.confirm, tapModeOf(opt));
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

  form.append(labelWrap, cmdWrap, tapWrap, optionsLabel, placeWrap, colorRow, confirmRow, actions);
  form.addEventListener("submit", (ev) => {
    ev.preventDefault();
    const label = labelInput.value.trim();
    const command = encodeTapMode(cmdInputEl.value.trim(), tapModeIn.value);
    const options = optionRows
      .map((row) => ({
        label: row.labelIn.value.trim(),
        command: encodeTapMode(row.cmdIn.value.trim(), row.tapIn.value),
        confirm: row.confirmIn.checked,
        insert: row.tapIn.value !== "send",
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
        insert: tapModeIn.value !== "send",
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

// Send whatever is in the input: the Send button, hardware enter, and
// type-in macros ending in \r all land here. The echo comes back over
// the text stream, so nothing renders locally.
function submitInput() {
  const text = cmdInput.value.trim();
  if (!text) return;
  sendCommand(text);
  recordHistory(text);
  cmdInput.value = "";
}

inputForm.addEventListener("submit", (ev) => {
  ev.preventDefault();
  submitInput();
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
// Story-text size, adjusted live from a stepper sheet. Roaming pref:
// localStorage is the offline fallback, a server-side web.story_size
// wins on connect, and stepper changes push back to the character
// profile. (A phone and a tablet sharing a character will fight over
// it; per-device divergence is what the unset server value preserves.)

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
    roamPut("web.story_size", storySize);
  });
  bigger.addEventListener("click", () => {
    storySize += 1;
    applyStorySize();
    refresh();
    roamPut("web.story_size", storySize);
  });
  refresh();
  stepper.append(smaller, value, bigger);
  sheetItems.appendChild(stepper);
  sheetNote("Changes apply live — tap anywhere else to close", true);
});

// ---- Soft keyboard / viewport --------------------------------------------

// Pin the app to the *visual* viewport so the soft keyboard never covers
// the input bar (iOS Safari doesn't resize the layout viewport). Safari
// reveals a focused input by moving the viewport rather than resizing,
// and does it two different ways: iPhones offset the visual viewport
// inside the layout viewport (vv.offsetTop), iPads scroll the document
// itself (window.scrollY, offsetTop stays 0). vv.pageTop is the sum of
// both — the visual viewport's offset from the document origin — so
// following it keeps the app pinned on either device (stays 0 on
// platforms that resize instead).
const vv = window.visualViewport;
function syncViewport() {
  if (!vv) return;
  document.documentElement.style.setProperty("--vvh", `${vv.height}px`);
  document.documentElement.style.setProperty("--vvt", `${vv.pageTop}px`);
  if (autoScroll) scrollToBottom();
}
if (vv) {
  vv.addEventListener("resize", syncViewport);
  vv.addEventListener("scroll", syncViewport);
  // Document scrolls (the iPad path) don't reliably fire visualViewport
  // events — they move pageTop without touching offsetTop.
  window.addEventListener("scroll", syncViewport);
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

// ---- Side drawers: macro tray (left) + status/injuries (right) ------------
// Swipe in from either edge (starting inside the screen so Android's back
// gesture keeps the literal edge) or tap the handles; text keeps flowing
// beneath the translucent panel. Swipe back or tap the pane to close.

const drawerLeft = document.getElementById("drawer-left");
const drawerRight = document.getElementById("drawer-right");
const paneWrap = document.getElementById("pane-wrap");
let injuries = {};
let targets = [];
let charInfo = {};

function setCharInfo(info) {
  charInfo = info || {};
  if (drawerRight.classList.contains("open")) renderStatusDrawer();
}

function setTargets(list) {
  targets = list || [];
  if (drawerRight.classList.contains("open")) renderStatusDrawer();
}

// Tap-to-target rides the ordinary link machinery: the server resolves
// the creature's menu (attack, look, target, ...) exactly like tapping
// its name in the story text.
function tapCreature(t) {
  if (!state.ws || state.ws.readyState !== WebSocket.OPEN) return;
  const requestId = ++menuRequestCounter;
  pendingMenuRequest = requestId;
  openSheetLoading(t.noun || t.name);
  state.ws.send(JSON.stringify({
    t: "link_tap",
    d: {
      request_id: requestId,
      exist_id: t.id,
      noun: t.noun || "",
      text: t.name,
      coord: null,
    },
  }));
}

function drawerOpen() {
  return drawerLeft.classList.contains("open") || drawerRight.classList.contains("open");
}

function openDrawer(side) {
  closeDrawers();
  (side === "left" ? drawerLeft : drawerRight).classList.add("open");
  if (side === "left") renderTray();
  else renderStatusDrawer();
}

function closeDrawers() {
  drawerLeft.classList.remove("open");
  drawerRight.classList.remove("open");
}

document.getElementById("handle-left").addEventListener("click", () => openDrawer("left"));
document.getElementById("handle-right").addEventListener("click", () => openDrawer("right"));
for (const btn of document.querySelectorAll(".drawer-close")) {
  btn.addEventListener("click", closeDrawers);
}

// Edge swipes. Track a touch that starts in the outer 48px of the pane
// (drawer closed) or anywhere (drawer open, for swipe-to-close).
let swipe = null;
paneWrap.addEventListener("touchstart", (ev) => {
  if (ev.touches.length !== 1) { swipe = null; return; }
  // The compass can be parked in an edge zone; touches on it are taps or
  // compass drags, never drawer swipes.
  if (ev.target.closest && ev.target.closest("#compass")) { swipe = null; return; }
  const t = ev.touches[0];
  const rect = paneWrap.getBoundingClientRect();
  const x = t.clientX - rect.left;
  if (drawerOpen()) {
    swipe = { x0: t.clientX, y0: t.clientY, mode: "close" };
  } else if (x < 48) {
    swipe = { x0: t.clientX, y0: t.clientY, mode: "open-left" };
  } else if (x > rect.width - 48) {
    swipe = { x0: t.clientX, y0: t.clientY, mode: "open-right" };
  } else {
    swipe = null;
  }
}, { passive: true });

paneWrap.addEventListener("touchmove", (ev) => {
  if (!swipe) return;
  const t = ev.touches[0];
  const dx = t.clientX - swipe.x0;
  const dy = t.clientY - swipe.y0;
  if (Math.abs(dy) > Math.abs(dx) * 1.5) { swipe = null; return; } // vertical scroll
  if (swipe.mode === "open-left" && dx > 56) { openDrawer("left"); swipe = null; }
  else if (swipe.mode === "open-right" && dx < -56) { openDrawer("right"); swipe = null; }
  else if (swipe.mode === "close") {
    if ((drawerLeft.classList.contains("open") && dx < -56) ||
        (drawerRight.classList.contains("open") && dx > 56)) {
      closeDrawers();
      swipe = null;
    }
  }
}, { passive: true });
paneWrap.addEventListener("touchend", () => { swipe = null; }, { passive: true });

// ---- Compass position ------------------------------------------------------
// Same feel as the floating macro buttons: a quick tap on a lit direction
// still walks; holding ~half a second lifts the whole compass to drag it.
// The position (compass center, as pane fractions) persists per device.

const COMPASS_POS_KEY = "vellum-compass-pos";
const compassEl = document.getElementById("compass");

function placeCompass(cx, cy) {
  compassEl.style.left = `${cx * 100}%`;
  compassEl.style.top = `${cy * 100}%`;
  compassEl.style.right = "auto";
  compassEl.style.bottom = "auto";
  compassEl.style.transform = "translate(-50%, -50%)";
}

try {
  const pos = JSON.parse(localStorage.getItem(COMPASS_POS_KEY) || "null");
  if (Array.isArray(pos)) placeCompass(pos[0], pos[1]);
} catch { /* corrupt entry — default corner */ }

(() => {
  let holdTimer = null;
  let dragging = false;
  compassEl.addEventListener("pointerdown", (ev) => {
    dragging = false;
    clearTimeout(holdTimer);
    holdTimer = setTimeout(() => {
      dragging = true;
      compassEl.classList.add("dragging");
      try {
        compassEl.setPointerCapture(ev.pointerId);
      } catch { /* pointer already gone */ }
    }, 450);
  });
  compassEl.addEventListener("pointermove", (ev) => {
    if (!dragging) return;
    const rect = paneWrap.getBoundingClientRect();
    const halfW = compassEl.offsetWidth / 2;
    const halfH = compassEl.offsetHeight / 2;
    const px = Math.min(rect.width - halfW - 4, Math.max(halfW + 4, ev.clientX - rect.left));
    const py = Math.min(rect.height - halfH - 4, Math.max(halfH + 4, ev.clientY - rect.top));
    const cx = px / rect.width;
    const cy = py / rect.height;
    placeCompass(cx, cy);
    compassEl.dataset.cx = cx;
    compassEl.dataset.cy = cy;
  });
  const endDrag = () => {
    clearTimeout(holdTimer);
    if (dragging && compassEl.dataset.cx) {
      try {
        localStorage.setItem(
          COMPASS_POS_KEY,
          JSON.stringify([parseFloat(compassEl.dataset.cx), parseFloat(compassEl.dataset.cy)]),
        );
      } catch { /* storage blocked — position just won't persist */ }
    }
    compassEl.classList.remove("dragging");
    // Cleared on a timeout so the click that follows pointerup still sees
    // dragging=true and gets swallowed below.
    setTimeout(() => {
      dragging = false;
    }, 0);
  };
  compassEl.addEventListener("pointerup", endDrag);
  compassEl.addEventListener("pointercancel", endDrag);
  compassEl.addEventListener("contextmenu", (ev) => ev.preventDefault());
  // Capture phase: the tap that ends a drag must not walk the character.
  compassEl.addEventListener(
    "click",
    (ev) => {
      if (dragging) {
        ev.preventDefault();
        ev.stopPropagation();
      }
    },
    true,
  );
})();

// Tapping the visible pane while a drawer is open closes it (capture phase
// so the tap never reaches links beneath).
paneWrap.addEventListener("click", (ev) => {
  if (!drawerOpen()) return;
  if (ev.target.closest(".drawer") || ev.target.closest(".drawer-handle")) return;
  ev.stopPropagation();
  ev.preventDefault();
  closeDrawers();
}, true);

// ---- Left drawer: vertical macro tray --------------------------------------

function renderTray() {
  const tray = document.getElementById("tray-content");
  tray.replaceChildren();
  if (!macros || (!macros.groups.length && !macros.floating.length)) {
    tray.appendChild(Object.assign(document.createElement("p"), {
      className: "drawer-empty",
      textContent: "No macros yet — use the + button on the macro rail to create one.",
    }));
    return;
  }
  for (const group of macros.groups) {
    const head = document.createElement("div");
    head.className = "tray-group";
    head.textContent = group.name;
    tray.appendChild(head);
    for (const btn of orderedButtons(group)) {
      const row = document.createElement("div");
      row.className = "tray-row";
      const el = document.createElement("button");
      el.type = "button";
      el.className = "tray-btn";
      el.textContent = btn.label;
      if (btn.color) el.style.borderLeftColor = btn.color;
      el.addEventListener("click", () => activateMacro(btn));
      const more = document.createElement("button");
      more.type = "button";
      more.className = "tray-more";
      more.setAttribute("aria-label", `Edit or move ${btn.label}`);
      more.textContent = "⋯";
      more.addEventListener("click", () => openMacroArrange(group, btn));
      row.append(el, more);
      tray.appendChild(row);
    }
  }
  const add = document.createElement("button");
  add.type = "button";
  add.className = "tray-add";
  add.textContent = "＋ Add or edit macros";
  add.addEventListener("click", () => document.getElementById("macro-add").click());
  tray.appendChild(add);
}

// ---- Right drawer: injury doll + status -------------------------------------

const LEVEL_COLORS = {
  1: "#d9b44f", 2: "#e08a2e", 3: "#d9534f", // wounds
  4: "#8a8f98", 5: "#a98f6e", 6: "#7a5c49", // scars
};

// Viewer-mirrored like the game's doll: the character's left side draws on
// the viewer's right. `back` and `nsys` get indicator chips, not geometry.
const DOLL_PARTS = [
  ["head", "<circle cx='70' cy='16' r='13'/>"],
  ["rightEye", "<circle cx='64' cy='13' r='3'/>"],
  ["leftEye", "<circle cx='76' cy='13' r='3'/>"],
  ["neck", "<rect x='63' y='29' width='14' height='7' rx='2'/>"],
  ["chest", "<rect x='50' y='36' width='40' height='26' rx='3'/>"],
  ["abdomen", "<rect x='52' y='62' width='36' height='20' rx='3'/>"],
  ["rightArm", "<rect x='33' y='38' width='14' height='42' rx='6'/>"],
  ["leftArm", "<rect x='93' y='38' width='14' height='42' rx='6'/>"],
  ["rightHand", "<rect x='33' y='82' width='14' height='12' rx='4'/>"],
  ["leftHand", "<rect x='93' y='82' width='14' height='12' rx='4'/>"],
  ["rightLeg", "<rect x='52' y='84' width='16' height='46' rx='6'/>"],
  ["leftLeg", "<rect x='72' y='84' width='16' height='46' rx='6'/>"],
];

const PART_LABELS = {
  head: "head", neck: "neck", chest: "chest", abdomen: "abdomen",
  back: "back", leftArm: "left arm", rightArm: "right arm",
  leftHand: "left hand", rightHand: "right hand", leftLeg: "left leg",
  rightLeg: "right leg", leftEye: "left eye", rightEye: "right eye",
  nsys: "nerves",
};

function injuryText(level) {
  return level <= 3 ? `wound ${level}` : `scar ${level - 3}`;
}

// ---- Skinned doll: base art + generated dots --------------------------------
// Mirrors the GUI skin system: /doll.json says whether the active skin
// ships doll base art, where every part's calibrated anchor sits
// (fractions of the image), how dots are styled, and which part/severity
// combos have hand-drawn overlays. Anchors being fractions means the SVG
// scales freely — same coordinates as the desktop, any display size.

let dollSkin = null;      // /doll.json payload when base art is active
let dollNatural = null;   // { w, h } of the base image, measured client-side

function dollImageUrl(kind, part, level) {
  let url = `/doll/image?kind=${kind}&token=${encodeURIComponent(pairingToken)}`;
  if (part) url += `&part=${encodeURIComponent(part)}&level=${level}`;
  return url;
}

function fetchDollSkin() {
  fetch(`/doll.json?token=${encodeURIComponent(pairingToken)}`)
    .then((r) => (r.ok ? r.json() : null))
    .then((d) => {
      dollSkin = d && d.base ? d : null;
      dollNatural = null;
      if (!dollSkin) {
        if (drawerRight.classList.contains("open")) renderStatusDrawer();
        return;
      }
      // The anchors are fractions of the image, so the client needs its
      // natural size to build the SVG viewBox; measure it here rather
      // than shipping image-decoding on the server.
      const img = new Image();
      img.onload = () => {
        dollNatural = { w: img.naturalWidth, h: img.naturalHeight };
        if (drawerRight.classList.contains("open")) renderStatusDrawer();
      };
      img.onerror = () => { dollSkin = null; };
      img.src = dollImageUrl("base");
    })
    .catch(() => { dollSkin = null; });
}

// Only pass through colors we can trust inside SVG markup.
function safeColor(value, fallback) {
  return /^#[0-9a-f]{6}$/i.test(value || "") ? value : fallback;
}

function dotNumeralColor(hex) {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return 0.299 * r + 0.587 * g + 0.114 * b > 140 ? "#000000" : "#ffffff";
}

// SVG for the skinned doll: base image, then per injured part either the
// skin's overlay art (stacked full-canvas, like the GUI) or a generated
// dot at the anchor — solid circle + numeral for wounds, ring for scars.
function skinDollSvg() {
  const { w, h } = dollNatural;
  const dots = dollSkin.dots || {};
  const woundColor = safeColor(dots.wound_color, "#e02020");
  const scarColor = safeColor(dots.scar_color, "#b8b8b8");
  const opacity = typeof dots.opacity === "number" ? Math.min(Math.max(dots.opacity, 0), 1) : 0.9;
  const r = Math.max(((dots.diameter || 0.07) * h) / 2, 3);
  const fontSize = Math.max(r * 1.3, 8);
  const parts = [`<image href="${dollImageUrl("base")}" width="${w}" height="${h}"/>`];
  for (const [id, level] of Object.entries(injuries).sort()) {
    if (!level || level > 6) continue;
    if (((dollSkin.overlays || {})[id] || []).includes(level)) {
      parts.push(
        `<image href="${dollImageUrl("overlay", id, level)}" width="${w}" height="${h}"/>`
      );
      continue;
    }
    const anchor = (dollSkin.anchors || {})[id];
    if (!anchor) continue; // unknown part: the injured list below still names it
    const cx = anchor[0] * w;
    const cy = anchor[1] * h;
    const numeral = level <= 3 ? level : level - 3;
    const text = (fill) =>
      `<text x="${cx}" y="${cy}" fill="${fill}" font-size="${fontSize}" ` +
      `font-weight="700" text-anchor="middle" dominant-baseline="central">${numeral}</text>`;
    if (level <= 3) {
      parts.push(
        `<g opacity="${opacity}"><circle cx="${cx}" cy="${cy}" r="${r}" fill="${woundColor}"/>` +
        text(dotNumeralColor(woundColor)) + `</g>`
      );
    } else {
      const strokeWidth = Math.max(r * 0.28, 1.2);
      parts.push(
        `<g opacity="${opacity}"><circle cx="${cx}" cy="${cy}" r="${r}" fill="none" ` +
        `stroke="${scarColor}" stroke-width="${strokeWidth}"/>` +
        text(scarColor) + `</g>`
      );
    }
  }
  return `<svg viewBox="0 0 ${w} ${h}" role="img" aria-label="Injury doll">${parts.join("")}</svg>`;
}

function setInjuries(map) {
  injuries = map || {};
  if (drawerRight.classList.contains("open")) renderStatusDrawer();
}

function renderStatusDrawer() {
  const panel = document.getElementById("status-content");
  panel.replaceChildren();

  // Targets first: mid-combat is when this drawer earns its keep.
  if (targets.length) {
    panel.appendChild(sectionTitle("Targets"));
    const section = document.createElement("div");
    section.className = "status-section";
    for (const t of targets) {
      const row = document.createElement("button");
      row.type = "button";
      row.className = "status-row target-row";
      if (t.current) row.classList.add("target-current");
      const name = document.createElement("span");
      name.textContent = (t.current ? "▸ " : "") + t.name;
      row.appendChild(name);
      if (t.status) {
        const status = document.createElement("span");
        status.className = "status-time";
        status.textContent = t.status;
        row.appendChild(status);
      }
      row.addEventListener("click", () => tapCreature(t));
      section.appendChild(row);
    }
    panel.appendChild(section);
  }

  // Players in the room: a standalone "who's here" list. The data already
  // rides RemoteDelta::Entities (interact mode consumes it too); each row
  // opens the player's noun menu via the same link-tap path as a target.
  if (roomEntities.players.length) {
    panel.appendChild(sectionTitle("Players"));
    const section = document.createElement("div");
    section.className = "status-section";
    for (const p of roomEntities.players) {
      const row = document.createElement("button");
      row.type = "button";
      row.className = "status-row target-row";
      const name = document.createElement("span");
      name.textContent = p.label;
      row.appendChild(name);
      row.addEventListener("click", () =>
        tapCreature({ id: p.id, noun: p.noun, name: p.label }));
      section.appendChild(row);
    }
    panel.appendChild(section);
  }

  // Doll: skinned (base art + dots at calibrated anchors) when the active
  // skin ships doll art, the built-in vector figure otherwise.
  const skinned = !!(dollSkin && dollNatural);
  const doll = document.createElement("div");
  doll.id = "status-doll";
  if (skinned) {
    doll.classList.add("skinned");
    doll.innerHTML = skinDollSvg();
  } else {
    const shapes = DOLL_PARTS.map(([id, shape]) => {
      const level = injuries[id] || 0;
      const fill = LEVEL_COLORS[level] || "var(--border)";
      return shape.replace("/>", ` fill="${fill}"/>`);
    }).join("");
    doll.innerHTML =
      `<svg viewBox="0 0 140 132" role="img" aria-label="Injury doll">${shapes}</svg>`;
  }
  panel.appendChild(doll);

  // Back / nervous system indicator chips: vector mode only — the skinned
  // doll gives those parts real anchor positions.
  if (!skinned) {
    const chipRow = document.createElement("div");
    chipRow.id = "status-chiprow";
    for (const id of ["back", "nsys"]) {
      const level = injuries[id] || 0;
      if (!level) continue;
      const chip = document.createElement("span");
      chip.className = "status-chip";
      chip.style.borderColor = LEVEL_COLORS[level];
      chip.textContent = `${PART_LABELS[id]}: ${injuryText(level)}`;
      chipRow.appendChild(chip);
    }
    if (chipRow.children.length) panel.appendChild(chipRow);
  }

  // Injured-part list (exact severities beat squinting at colors).
  const injured = Object.entries(injuries).sort();
  if (injured.length) {
    panel.appendChild(sectionTitle("Injuries"));
    const list = document.createElement("div");
    list.className = "status-section";
    for (const [id, level] of injured) {
      const row = document.createElement("div");
      row.className = "status-row";
      const dot = document.createElement("span");
      dot.className = "hl-swatch";
      dot.style.background = LEVEL_COLORS[level] || "transparent";
      row.append(dot, ` ${PART_LABELS[id] || id}: ${injuryText(level)}`);
      list.appendChild(row);
    }
    panel.appendChild(list);
  } else {
    const ok = document.createElement("p");
    ok.className = "drawer-empty";
    ok.textContent = "No injuries.";
    panel.appendChild(ok);
  }

  // Hands
  panel.appendChild(sectionTitle("Hands"));
  const hands = document.createElement("div");
  hands.className = "status-section";
  for (const [tag, id] of [["L", "hand-left"], ["R", "hand-right"]]) {
    const row = document.createElement("div");
    row.className = "status-row";
    row.textContent = `${tag}: ${document.getElementById(id).textContent}`;
    hands.appendChild(row);
  }
  panel.appendChild(hands);

  // Character sheet (pre-formatted lines from the core).
  const CHAR_SECTIONS = [
    ["experience", "Experience"],
    ["encumbrance", "Encumbrance"],
    ["bounty", "Bounty"],
    ["society", "Society"],
  ];
  for (const [key, label] of CHAR_SECTIONS) {
    const lines = charInfo[key];
    if (!lines || !lines.length) continue;
    panel.appendChild(sectionTitle(label));
    const section = document.createElement("div");
    section.className = "status-section";
    for (const line of lines) {
      const row = document.createElement("div");
      row.className = "status-row status-wrap";
      row.textContent = line;
      section.appendChild(row);
    }
    panel.appendChild(section);
  }

  // Active effects with countdowns
  for (const cat of effectCategories) {
    if (!cat.effects.length) continue;
    panel.appendChild(sectionTitle(CATEGORY_LABELS[cat.category] || cat.category));
    const section = document.createElement("div");
    section.className = "status-section";
    for (const effect of cat.effects) {
      const row = document.createElement("div");
      row.className = "status-row status-effect";
      const name = document.createElement("span");
      name.textContent = effect.text;
      const time = document.createElement("span");
      time.className = "status-time";
      time.dataset.expires = effect.expiresAt ?? "";
      time.textContent =
        effect.expiresAt === null ? "" : fmtRemaining(effect.expiresAt - Date.now());
      row.append(name, time);
      section.appendChild(row);
    }
    panel.appendChild(section);
  }
}

function sectionTitle(text) {
  const el = document.createElement("div");
  el.className = "status-title";
  el.textContent = text;
  return el;
}

// Tick the visible countdowns once a second alongside the pill timer.
setInterval(() => {
  if (!drawerRight.classList.contains("open")) return;
  for (const el of document.querySelectorAll(".status-time")) {
    if (el.dataset.expires) {
      el.textContent = fmtRemaining(Number(el.dataset.expires) - Date.now());
    }
  }
}, 1000);

// Debug/test handle: the module scope hides everything from the console
// and from UI test drivers; this exposes the state setters (display-only
// helpers, no privileged actions) for both.
window.vellumDebug = {
  setRoom, setTargets, setCharInfo, setInjuries, setEffects, setSession,
};


// ---- Colors editor ----------------------------------------------------------
// Structured editing of colors.toml: preset colors (what the text engine
// references for speech/whispers/etc.) and prompt-character colors, with
// native pickers. Sections the UI does not cover (TUI chrome, spell
// colors, palette) live in the fetched document and survive the save.

const colorsOverlay = document.getElementById("colors-overlay");
const colorsList = document.getElementById("colors-list");
const colorsStatus = document.getElementById("colors-status");
let colorsScope = null;
let colorsDoc = null; // the full fetched ColorConfig JSON
let colorsRequestCounter = 0;
let colorsPendingRequest = null;

function colorsStatusMsg(text, isError) {
  colorsStatus.textContent = text;
  colorsStatus.classList.toggle("editor-error", !!isError);
  colorsStatus.hidden = !text;
}

function openColorsEditor(scope) {
  colorsScope = scope;
  colorsDoc = null;
  document.getElementById("colors-title").textContent =
    scope === "global" ? "Colors (global)" : "Colors (this profile)";
  colorsList.replaceChildren(Object.assign(document.createElement("p"), {
    className: "hl-empty", textContent: "Loading\u2026",
  }));
  colorsStatusMsg("", false);
  colorsOverlay.hidden = false;
  colorsPendingRequest = ++colorsRequestCounter;
  sendJson("colors_get", { request_id: colorsPendingRequest, scope });
}

function handleColorsReply(d) {
  if (d.request_id !== colorsPendingRequest) return;
  if (d.error) {
    colorsStatusMsg(d.error, true);
    return;
  }
  if (d.saved) {
    colorsStatusMsg("Saved \u2014 applied live.", false);
    return;
  }
  colorsDoc = d.colors || {};
  renderColorsList();
}

// One row: label + fg/bg pickers with clear buttons (select-on-tap, same
// lesson as the highlight form: picking the shown color fires no event).
function colorRow(label, obj, fgKey, bgKey) {
  const row = document.createElement("div");
  row.className = "color-row";
  const name = document.createElement("span");
  name.className = "color-row-name";
  name.textContent = label;
  row.appendChild(name);

  const makePicker = (key) => {
    const wrap = document.createElement("span");
    wrap.className = "color-cell";
    const pick = document.createElement("input");
    pick.type = "color";
    const current = obj[key];
    if (current && /^#[0-9a-f]{6}$/i.test(current)) pick.value = current;
    pick.classList.toggle("hl-inactive", !current);
    const none = document.createElement("button");
    none.type = "button";
    none.textContent = "\u2715";
    none.className = "color-none";
    none.classList.toggle("hl-none-active", !current);
    const setVal = (v) => {
      if (v) obj[key] = v; else delete obj[key];
      pick.classList.toggle("hl-inactive", !v);
      none.classList.toggle("hl-none-active", !v);
    };
    pick.addEventListener("click", () => setVal(pick.value));
    pick.addEventListener("input", () => setVal(pick.value));
    none.addEventListener("click", () => setVal(null));
    wrap.append(pick, none);
    return wrap;
  };

  row.appendChild(makePicker(fgKey));
  if (bgKey) row.appendChild(makePicker(bgKey));
  return row;
}

function colorsSection(title) {
  const el = document.createElement("div");
  el.className = "status-title";
  el.textContent = title;
  return el;
}

// A row with a single color picker bound to a string field (UI colors are
// plain hex strings, not fg/bg objects). Reuses the same picker affordance as
// colorRow so the whole sheet reads consistently.
function singleColorRow(label, obj, key) {
  const row = document.createElement("div");
  row.className = "color-row";
  const name = document.createElement("span");
  name.className = "color-row-name";
  name.textContent = label;
  row.appendChild(name);

  const wrap = document.createElement("span");
  wrap.className = "color-cell";
  const pick = document.createElement("input");
  pick.type = "color";
  const current = obj[key];
  const isHex = current && /^#[0-9a-f]{6}$/i.test(current);
  if (isHex) pick.value = current;
  pick.classList.toggle("hl-inactive", !current);
  const none = document.createElement("button");
  none.type = "button";
  none.textContent = "✕";
  none.className = "color-none";
  none.classList.toggle("hl-none-active", !current);
  const setVal = (v) => {
    if (v) obj[key] = v; else obj[key] = "";
    pick.classList.toggle("hl-inactive", !v);
    none.classList.toggle("hl-none-active", !v);
  };
  pick.addEventListener("click", () => setVal(pick.value));
  pick.addEventListener("input", () => setVal(pick.value));
  none.addEventListener("click", () => setVal(null));
  wrap.append(pick, none);
  row.appendChild(wrap);
  return row;
}

// A labeled section header with a "+ Add" button on the right.
function colorsSectionWithAdd(title, addLabel, onAdd) {
  const wrap = document.createElement("div");
  wrap.className = "color-section-head";
  const t = document.createElement("span");
  t.className = "status-title";
  t.textContent = title;
  const add = document.createElement("button");
  add.type = "button";
  add.className = "color-add";
  add.textContent = addLabel;
  add.addEventListener("click", onAdd);
  wrap.append(t, add);
  return wrap;
}

// A deletable row wrapper: appends a small remove button that splices `arr`
// at `index` and re-renders.
function withDelete(row, arr, index) {
  const del = document.createElement("button");
  del.type = "button";
  del.className = "color-del";
  del.textContent = "🗑"; // wastebasket
  del.setAttribute("aria-label", "Delete");
  del.addEventListener("click", () => {
    arr.splice(index, 1);
    renderColorsList();
  });
  row.appendChild(del);
  return row;
}

function renderColorsList() {
  colorsList.replaceChildren();
  const doc = colorsDoc || {};

  const presets = doc.presets || {};
  const names = Object.keys(presets).sort((a, b) => a.localeCompare(b));
  if (names.length) {
    colorsList.appendChild(colorsSection("Text presets (color \u00b7 background)"));
    for (const name of names) {
      colorsList.appendChild(colorRow(name, presets[name], "fg", "bg"));
    }
  }

  const prompts = (doc.prompt_colors = doc.prompt_colors || []);
  colorsList.appendChild(
    colorsSectionWithAdd("Prompt characters", "+ Add", () => {
      prompts.push({ character: ">", fg: null, bg: null });
      renderColorsList();
    }),
  );
  prompts.forEach((p, i) => {
    const label = document.createElement("input");
    label.className = "color-key-input";
    label.value = p.character || "";
    label.maxLength = 4;
    label.setAttribute("aria-label", "Prompt character");
    label.addEventListener("input", () => { p.character = label.value; });
    const row = colorRow("", p, "fg", "bg");
    row.querySelector(".color-row-name").replaceWith(label);
    colorsList.appendChild(withDelete(row, prompts, i));
  });

  // UI chrome colors (plain hex strings on doc.ui). One picker each.
  const ui = doc.ui;
  if (ui) {
    colorsList.appendChild(colorsSection("Interface colors"));
    const UI_FIELDS = [
      ["command_echo_color", "Command echo"],
      ["border_color", "Border"],
      ["focused_border_color", "Focused border"],
      ["text_color", "Text"],
      ["background_color", "Background"],
      ["selection_bg_color", "Selection background"],
      ["textarea_background", "Textarea background"],
    ];
    for (const [key, label] of UI_FIELDS) {
      colorsList.appendChild(singleColorRow(label, ui, key));
    }
  }

  // Spell color ranges: spell ids + bar/text/bg pickers.
  const spells = (doc.spell_colors = doc.spell_colors || []);
  colorsList.appendChild(
    colorsSectionWithAdd("Spell colors (bar · text · background)", "+ Add", () => {
      spells.push({ spells: [], color: "", bar_color: null, text_color: null, bg_color: null });
      renderColorsList();
    }),
  );
  spells.forEach((s, i) => {
    const ids = document.createElement("input");
    ids.className = "color-key-input color-ids-input";
    ids.value = (s.spells || []).join(", ");
    ids.setAttribute("aria-label", "Spell IDs (comma-separated)");
    ids.placeholder = "101, 107";
    ids.addEventListener("input", () => {
      s.spells = ids.value
        .split(",")
        .map((t) => parseInt(t.trim(), 10))
        .filter((n) => Number.isFinite(n));
    });
    // Three pickers: bar_color, text_color, bg_color.
    const row = document.createElement("div");
    row.className = "color-row";
    row.appendChild(ids);
    for (const key of ["bar_color", "text_color", "bg_color"]) {
      const cell = colorRow("", s, key, null);
      row.appendChild(cell.querySelector(".color-cell"));
    }
    colorsList.appendChild(withDelete(row, spells, i));
  });

  // Palette entries: name + single color + category.
  const palette = (doc.color_palette = doc.color_palette || []);
  colorsList.appendChild(
    colorsSectionWithAdd("Palette", "+ Add", () => {
      palette.push({ name: "", color: "#ffffff", category: "", favorite: false });
      renderColorsList();
    }),
  );
  palette.forEach((c, i) => {
    const row = document.createElement("div");
    row.className = "color-row";
    const nameInput = document.createElement("input");
    nameInput.className = "color-key-input";
    nameInput.value = c.name || "";
    nameInput.placeholder = "name";
    nameInput.setAttribute("aria-label", "Palette color name");
    nameInput.addEventListener("input", () => { c.name = nameInput.value; });
    const catInput = document.createElement("input");
    catInput.className = "color-key-input color-cat-input";
    catInput.value = c.category || "";
    catInput.placeholder = "category";
    catInput.setAttribute("aria-label", "Palette color category");
    catInput.addEventListener("input", () => { c.category = catInput.value; });
    row.append(nameInput);
    const picker = singleColorRow("", c, "color");
    row.appendChild(picker.querySelector(".color-cell"));
    row.append(catInput);
    colorsList.appendChild(withDelete(row, palette, i));
  });
}

document.getElementById("colors-save").addEventListener("click", () => {
  if (!colorsDoc) return;
  colorsStatusMsg("Saving\u2026", false);
  colorsPendingRequest = ++colorsRequestCounter;
  sendJson("colors_put", {
    request_id: colorsPendingRequest,
    scope: colorsScope,
    colors: colorsDoc,
  });
});

document.getElementById("colors-close").addEventListener("click", () => {
  colorsOverlay.hidden = true;
  colorsScope = null;
  colorsDoc = null;
});

// ---- Client settings (registry-backed, saved on the host) ------------------
// The server dumps its settings registry (settings_get: key, label, kind,
// scope, live value) and this sheet renders it: collapsible category
// groups, a per-kind input per row, a per-row Character/Global scope, and
// save-on-change via settings_put (debounced for typed fields — same
// live-apply idiom as the Appearance sheet, no big Save button to forget).
// Unlike Appearance (per-phone localStorage), these live in the connected
// character/host profile. Sensitive settings arrive redacted and are only
// sent when the user types a new value.

const csOverlay = document.createElement("div");
csOverlay.id = "cs-overlay";
csOverlay.hidden = true;
csOverlay.innerHTML = `
  <div id="cs-card">
    <div id="cs-titlebar">
      <span id="cs-title">Client settings</span>
      <button type="button" id="cs-close">Close</button>
    </div>
    <p id="cs-tier-note">These save to the connected character/host profile.
      Appearance above is per-phone.</p>
    <div id="cs-list"></div>
  </div>`;
document.body.appendChild(csOverlay);
const csList = csOverlay.querySelector("#cs-list");

let csCatalog = null;
let csRequestCounter = 0;
let csPendingGet = null;
const csPendingPuts = new Map(); // request_id -> { statusEl, onSaved }

csOverlay.querySelector("#cs-close").addEventListener("click", () => {
  csOverlay.hidden = true;
  csPendingGet = null;
});

function openClientSettings() {
  csOverlay.hidden = false;
  csList.replaceChildren(Object.assign(document.createElement("p"), {
    className: "hl-empty", textContent: "Loading…",
  }));
  csPendingGet = ++csRequestCounter;
  sendJson("settings_get", { request_id: csPendingGet });
}

function handleSettingsReply(d) {
  // Roaming-pref traffic shares the settings wire but never touches the
  // sheet UI: gets apply silently, puts are fire-and-forget.
  if (d.request_id === roamPendingGet) {
    roamPendingGet = null;
    if (!d.error && Array.isArray(d.catalog)) applyRoamingPrefs(d.catalog);
    return;
  }
  if (roamPendingPuts.delete(d.request_id)) {
    if (d.error) console.debug("roaming pref save failed:", d.error);
    return;
  }
  if (d.request_id === csPendingGet) {
    csPendingGet = null;
    if (d.error) {
      csList.replaceChildren(Object.assign(document.createElement("p"), {
        className: "hl-empty editor-error", textContent: d.error,
      }));
      return;
    }
    csCatalog = Array.isArray(d.catalog) ? d.catalog : [];
    renderClientSettings();
    return;
  }
  const pending = csPendingPuts.get(d.request_id);
  if (!pending) return;
  csPendingPuts.delete(d.request_id);
  clearTimeout(pending.timeout);
  if (d.error) {
    csRowStatus(pending.statusEl, d.error, true);
  } else {
    if (pending.onSaved) pending.onSaved();
    csRowStatus(pending.statusEl, "Saved", false);
    setTimeout(() => {
      if (pending.statusEl.textContent === "Saved") csRowStatus(pending.statusEl, "", false);
    }, 1500);
  }
}

function csRowStatus(el, text, isError) {
  el.textContent = text;
  el.classList.toggle("editor-error", !!isError);
}

function csSendPut(entry, value, scope, statusEl, onSaved, clear) {
  csRowStatus(statusEl, "Saving…", false);
  const requestId = ++csRequestCounter;
  const pending = { statusEl, onSaved };
  // Never leave a row spinning if the reply is lost (disconnect).
  pending.timeout = setTimeout(() => {
    if (csPendingPuts.delete(requestId)) csRowStatus(statusEl, "No reply — retry", true);
  }, 8000);
  csPendingPuts.set(requestId, pending);
  sendJson("settings_put", {
    request_id: requestId,
    key: entry.key,
    value,
    scope,
    ...(clear ? { clear: true } : {}),
  });
}

// The current value a row's control holds, as the wire value for its kind,
// or null with an error shown when it doesn't parse.
function csControlValue(entry, control, statusEl) {
  const type = entry.kind.type;
  if (type === "bool") return control.checked;
  if (type === "int" || type === "float") {
    const n = Number(control.value);
    if (control.value.trim() === "" || !Number.isFinite(n)) {
      csRowStatus(statusEl, "Enter a number", true);
      return null;
    }
    if (type === "int" && !Number.isInteger(n)) {
      csRowStatus(statusEl, "Whole numbers only", true);
      return null;
    }
    return n;
  }
  if (type === "list") {
    return control.value.split("\n").map((s) => s.trim()).filter(Boolean);
  }
  return control.value; // text / optional_text / enum / sensitive
}

function csMakeControl(entry) {
  const type = entry.kind.type;
  if (type === "bool") {
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = !!entry.value;
    return input;
  }
  if (type === "int" || type === "float") {
    const input = document.createElement("input");
    input.type = "number";
    if (entry.kind.min !== undefined) input.min = String(entry.kind.min);
    if (entry.kind.max !== undefined) input.max = String(entry.kind.max);
    input.step = type === "int" ? "1" : "any";
    input.inputMode = type === "int" ? "numeric" : "decimal";
    input.value = String(entry.value);
    return input;
  }
  if (type === "enum") {
    const select = document.createElement("select");
    const options = entry.kind.options || [];
    if (!options.includes(entry.value)) select.appendChild(new Option(entry.value, entry.value));
    for (const opt of options) select.appendChild(new Option(opt, opt));
    select.value = entry.value;
    return select;
  }
  if (type === "list") {
    const area = document.createElement("textarea");
    area.rows = Math.min(6, Math.max(2, (entry.value || []).length + 1));
    area.value = (entry.value || []).join("\n");
    area.placeholder = "One entry per line";
    return area;
  }
  const input = document.createElement("input");
  if (entry.sensitive) {
    input.type = "password";
    input.autocomplete = "new-password";
    input.value = "";
    if (entry.redacted) input.placeholder = "(unchanged)";
  } else {
    input.type = "text";
    input.value = entry.value || "";
    if (entry.kind.type === "optional_text") input.placeholder = "(empty = default)";
  }
  return input;
}

function csRow(entry) {
  const row = document.createElement("div");
  row.className = "cs-row";

  const label = document.createElement("div");
  label.className = "cs-label";
  label.textContent = entry.label;
  const status = document.createElement("span");
  status.className = "cs-status";

  const controls = document.createElement("div");
  controls.className = "cs-control-row";
  const control = csMakeControl(entry);

  // Scope: per-row Character/Global choice, defaulting Character.
  // Character-only settings save to the character file regardless — show
  // a tag instead of a choice.
  let scopeSel = null;
  if (entry.scope === "character_only") {
    const tag = document.createElement("span");
    tag.className = "cs-tag";
    tag.textContent = "character";
    label.appendChild(tag);
  } else {
    scopeSel = document.createElement("select");
    scopeSel.className = "cs-scope";
    scopeSel.appendChild(new Option("Character", "character"));
    scopeSel.appendChild(new Option("Global", "global"));
    scopeSel.value = "character";
  }

  // A redacted secret can't be emptied by editing (the field never held
  // it), so it gets an explicit Clear that unsets the host-side value.
  let clearBtn = null;
  if (entry.sensitive && entry.redacted) {
    clearBtn = document.createElement("button");
    clearBtn.type = "button";
    clearBtn.className = "cs-clear";
    clearBtn.textContent = "Clear";
    clearBtn.addEventListener("click", () => {
      if (!confirm(`Clear the saved ${entry.label}?`)) return;
      csSendPut(entry, "", scopeSel ? scopeSel.value : "character", status, () => {
        entry.redacted = false;
        control.value = "";
        control.placeholder = "";
        clearBtn.hidden = true;
      }, true);
    });
  }

  const save = () => {
    // Sensitive: only send when the user actually typed a replacement.
    if (entry.sensitive && control.value === "") return;
    const value = csControlValue(entry, control, status);
    if (value === null && entry.kind.type !== "bool") return;
    const onSaved = entry.sensitive
      ? () => {
          entry.redacted = true;
          control.value = "";
          control.placeholder = "(unchanged)";
          if (clearBtn) clearBtn.hidden = false;
        }
      : null;
    csSendPut(entry, value, scopeSel ? scopeSel.value : "character", status, onSaved);
  };

  // Discrete controls commit on change; typed fields also save after a
  // typing pause so a hidden keyboard-blur isn't required.
  control.addEventListener("change", save);
  const typed = control.tagName === "TEXTAREA"
    || (control.tagName === "INPUT" && control.type !== "checkbox");
  if (typed) {
    let debounce = 0;
    control.addEventListener("input", () => {
      clearTimeout(debounce);
      debounce = setTimeout(save, 900);
    });
  }
  if (scopeSel) {
    // Re-picking scope re-saves the current value to the new file.
    scopeSel.addEventListener("change", save);
  }

  if (entry.kind.type === "bool") {
    // Checkbox rides inline with the label text.
    label.prepend(control);
    controls.append(...(scopeSel ? [scopeSel] : []), status);
  } else {
    controls.append(
      control,
      ...(clearBtn ? [clearBtn] : []),
      ...(scopeSel ? [scopeSel] : []),
      status,
    );
  }

  row.append(label);
  if (entry.description) {
    const desc = document.createElement("div");
    desc.className = "cs-desc";
    desc.textContent = entry.description;
    row.appendChild(desc);
  }
  row.appendChild(controls);
  return row;
}

function renderClientSettings() {
  csList.replaceChildren();
  const groups = new Map(); // category -> entries, in catalog order
  for (const entry of csCatalog || []) {
    if (!groups.has(entry.category)) groups.set(entry.category, []);
    groups.get(entry.category).push(entry);
  }
  if (!groups.size) {
    csList.appendChild(Object.assign(document.createElement("p"), {
      className: "hl-empty", textContent: "No settings received.",
    }));
    return;
  }
  for (const [category, entries] of [...groups.entries()].sort((a, b) =>
    a[0].localeCompare(b[0]))) {
    const group = document.createElement("details");
    group.className = "cs-group";
    const summary = document.createElement("summary");
    summary.textContent = `${category} (${entries.length})`;
    group.appendChild(summary);
    for (const entry of entries) group.appendChild(csRow(entry));
    csList.appendChild(group);
  }
}

// ---- Streams (routing, saved on the host) ----------------------------------
// The server ships the streams catalog (streams_get: every known stream,
// its effective destination, and whether a window subscribes) and this
// overlay renders it in the Client-settings idiom: one row per stream with
// per-row save-on-change status. Subscribed rows are read-only (window
// subscriptions are desktop-edited); orphan rows get a destination select
// that sends streams_put (Discard / Main / fallback / a window: route).

const stOverlay = document.createElement("div");
stOverlay.id = "st-overlay";
stOverlay.hidden = true;
stOverlay.innerHTML = `
  <div id="st-card">
    <div id="st-titlebar">
      <span id="st-title">Streams</span>
      <button type="button" id="st-close">Close</button>
    </div>
    <p id="st-note">Where each game text stream goes. Rows tagged
      "window" are claimed by a window subscription — edit those on the
      desktop. Everything else saves to the host as you change it.</p>
    <div id="st-list"></div>
  </div>`;
document.body.appendChild(stOverlay);
const stList = stOverlay.querySelector("#st-list");

let stCatalog = null;
let stRequestCounter = 0;
let stPendingGet = null;
const stPendingPuts = new Map(); // request_id -> { statusEl, timeout }

stOverlay.querySelector("#st-close").addEventListener("click", () => {
  stOverlay.hidden = true;
  stPendingGet = null;
});

function openStreamsPanel() {
  stOverlay.hidden = false;
  stList.replaceChildren(Object.assign(document.createElement("p"), {
    className: "hl-empty", textContent: "Loading…",
  }));
  stPendingGet = ++stRequestCounter;
  sendJson("streams_get", { request_id: stPendingGet });
}

function handleStreamsReply(d) {
  if (d.request_id === stPendingGet) {
    stPendingGet = null;
    if (d.error) {
      stList.replaceChildren(Object.assign(document.createElement("p"), {
        className: "hl-empty editor-error", textContent: d.error,
      }));
      return;
    }
    stCatalog = d;
    renderStreamsPanel();
    return;
  }
  const pending = stPendingPuts.get(d.request_id);
  if (!pending) return;
  stPendingPuts.delete(d.request_id);
  clearTimeout(pending.timeout);
  if (d.error) {
    csRowStatus(pending.statusEl, d.error, true);
  } else {
    csRowStatus(pending.statusEl, "Saved", false);
    setTimeout(() => {
      if (pending.statusEl.textContent === "Saved") csRowStatus(pending.statusEl, "", false);
    }, 1500);
  }
}

function stSendPut(stream, target, statusEl) {
  csRowStatus(statusEl, "Saving…", false);
  const requestId = ++stRequestCounter;
  const pending = { statusEl };
  // Never leave a row spinning if the reply is lost (disconnect).
  pending.timeout = setTimeout(() => {
    if (stPendingPuts.delete(requestId)) csRowStatus(statusEl, "No reply — retry", true);
  }, 8000);
  stPendingPuts.set(requestId, pending);
  sendJson("streams_put", { request_id: requestId, stream, target });
}

function stRow(entry, windows, fallback) {
  const row = document.createElement("div");
  row.className = "cs-row";
  const label = document.createElement("div");
  label.className = "cs-label";
  label.textContent = entry.id;
  const status = document.createElement("span");
  status.className = "cs-status";
  const controls = document.createElement("div");
  controls.className = "cs-control-row";

  if (entry.subscribed) {
    // A window subscription claims this stream; a route would only apply
    // if that window went away. Read-only from the phone.
    const tag = document.createElement("span");
    tag.className = "cs-tag";
    tag.textContent = "window";
    label.appendChild(tag);
    const dest = document.createElement("span");
    dest.className = "st-dest";
    dest.textContent = entry.destination;
    controls.append(dest, status);
  } else {
    const select = document.createElement("select");
    select.appendChild(new Option("Discard", "discard"));
    select.appendChild(new Option("Main", "main"));
    select.appendChild(new Option(`fallback (${fallback})`, "clear"));
    for (const name of windows) {
      select.appendChild(new Option(`window: ${name}`, `window:${name}`));
    }
    const current = entry.route || "clear";
    if (![...select.options].some((o) => o.value === current)) {
      // A window: route aimed at a window missing from the list still
      // shows (delivery falls back gracefully server-side).
      select.appendChild(new Option(current, current));
    }
    select.value = current;
    select.addEventListener("change", () => {
      stSendPut(entry.id, select.value, status);
    });
    controls.append(select, status);
  }

  row.append(label);
  if (entry.label) {
    // Friendly stream name from the Lich seen-streams registry.
    const desc = document.createElement("div");
    desc.className = "cs-desc";
    desc.textContent = entry.label;
    row.appendChild(desc);
  }
  row.appendChild(controls);
  return row;
}

function renderStreamsPanel() {
  stList.replaceChildren();
  const rows = (stCatalog && stCatalog.streams) || [];
  if (!rows.length) {
    stList.appendChild(Object.assign(document.createElement("p"), {
      className: "hl-empty",
      textContent: "No streams known yet — they appear as the game sends them.",
    }));
    return;
  }
  const group = document.createElement("div");
  group.className = "cs-group st-group";
  for (const entry of rows) {
    group.appendChild(stRow(entry, stCatalog.windows || [], stCatalog.fallback || "main"));
  }
  stList.appendChild(group);
}

// ---- Roaming phone prefs (per-character, server-side) ----------------------
// Story text size, theme, and chip order ride the character profile as
// registry settings (web.story_size / web.theme / web.chip_order) over
// the same settings_get/put wire as the sheet above, so switching phones
// keeps the look. Rules:
//   - a SET server value wins over this device's localStorage and
//     applies live on connect (and login for headless runtimes);
//   - an UNSET server value (0 / empty) changes nothing — localStorage
//     keeps working as before, and the local value is never pushed
//     uninvited (no surprise migration of a per-device pref);
//   - the server copy only changes when the user changes the pref here
//     (stepper / theme pick / chip arrange), debounced, character
//     scope, silent on success, and only with a session connected.
// Drag positions and the rest of Appearance stay per-device.

let roamPendingGet = null;
const roamPendingPuts = new Set(); // request_ids of in-flight silent puts
const roamPutTimers = new Map(); // setting key -> debounce timer

function fetchRoamingPrefs() {
  roamPendingGet = ++csRequestCounter;
  if (!sendJson("settings_get", { request_id: roamPendingGet })) {
    roamPendingGet = null;
  }
}

function applyRoamingPrefs(catalog) {
  const values = new Map();
  for (const entry of catalog) {
    values.set(entry.key, entry.value);
  }
  const size = values.get("web.story_size");
  if (Number.isInteger(size) && size > 0) {
    // applyStorySize clamps to this device's legible range and mirrors
    // the result into localStorage (the offline fallback tracks it).
    storySize = size;
    applyStorySize();
  }
  const theme = values.get("web.theme");
  if (typeof theme === "string" && theme && THEMES[theme]) {
    uiPrefs.theme = theme;
    saveUiPrefs();
    applyUiPrefs();
  }
  const order = values.get("web.chip_order");
  if (Array.isArray(order) && order.length) {
    chipOrder = order.filter((s) => typeof s === "string");
    try {
      localStorage.setItem(CHIP_ORDER_KEY, JSON.stringify(chipOrder));
    } catch { /* fine, just won't persist */ }
    applyChipOrder();
    updateChips();
  }
}

// Push one changed roaming pref to the character profile. Debounced per
// key (a stepper tap-tap-tap is one save), fire-and-forget: localStorage
// already holds the value, the server copy is the roaming bonus.
function roamPut(key, value) {
  clearTimeout(roamPutTimers.get(key));
  roamPutTimers.set(key, setTimeout(() => {
    roamPutTimers.delete(key);
    if (session.state !== "connected") return; // no session — stays local
    const requestId = ++csRequestCounter;
    roamPendingPuts.add(requestId);
    const sent = sendJson("settings_put", {
      request_id: requestId,
      key,
      value,
      scope: "character",
    });
    if (!sent) roamPendingPuts.delete(requestId);
  }, 600));
}

// ---- Map (full-screen canvas over the generated map scene) ----------------
// The server pushes the same sheet the desktop mini map draws: `map_scene`
// re-sent only when the drawn view changes (location / building / layout
// regeneration), plus a small `map_state` per step (current room, ghost
// sketches). Everything else is client-side: drag pans, pinch zooms, and
// tapping a room walks there via native .go2 — no Lich anywhere.

const mapOverlay = document.getElementById("map-overlay");
const mapCanvas = document.getElementById("map-canvas");
const mapBtn = document.getElementById("map-btn");
const mapTitleEl = document.getElementById("map-title");
const mapEmptyEl = document.getElementById("map-empty");
const mapFollowBtn = document.getElementById("map-follow");
const mapStopBtn = document.getElementById("map-stop");
const mapPicker = document.getElementById("map-picker");
const mapPickerFilter = document.getElementById("map-picker-filter");
const mapPickerList = document.getElementById("map-picker-list");
const mapPickerStatus = document.getElementById("map-picker-status");

let mapScene = null;
let mapState = {};
/// Browsing another location: { location, scene } — the live scene keeps
/// arriving underneath and is restored on Return.
let mapBrowse = null;
let mapCam = { x: 0, y: 0, ppc: 22 }; // center cell (fractional), px per cell
let mapFollow = true;
let mapRaf = 0;
let mapRequestCounter = 0;
let mapPendingRequest = 0;
let mapLocations = [];

function activeMapScene() {
  return mapBrowse ? mapBrowse.scene : mapScene;
}

function renderMapTitle() {
  if (mapBrowse) {
    mapTitleEl.textContent = `${mapBrowse.location} ▾`;
  } else if (mapScene) {
    mapTitleEl.textContent =
      mapScene.location +
      (mapScene.sheet === "interiors" ? " · inside" : "") +
      " ▾";
  } else {
    mapTitleEl.textContent = "Map ▾";
  }
  mapFollowBtn.textContent = mapBrowse ? "Return" : mapFollow ? "Following" : "Follow";
  mapStopBtn.hidden = !(mapState.travel && !mapBrowse);
}

function setMapScene(scene) {
  mapScene = scene;
  renderMapTitle();
  requestMapRender();
}

function setMapState(st) {
  mapState = st || {};
  mapBtn.hidden = !mapState.available;
  if (!mapBrowse && mapFollow && Array.isArray(mapState.cell)) {
    mapCam.x = mapState.cell[0];
    mapCam.y = mapState.cell[1];
  }
  renderMapTitle();
  requestMapRender();
}

function setMapFollow(on) {
  mapFollow = on;
  mapFollowBtn.classList.toggle("follow-on", on && !mapBrowse);
  if (on && !mapBrowse && Array.isArray(mapState.cell)) {
    mapCam.x = mapState.cell[0];
    mapCam.y = mapState.cell[1];
  }
  renderMapTitle();
  requestMapRender();
}

/// Leave browse mode: back to the live scene, following the character.
function exitMapBrowse() {
  mapBrowse = null;
  mapPicker.hidden = true;
  setMapFollow(true);
}

function openMapOverlay() {
  mapOverlay.hidden = false;
  resizeMapCanvas();
  renderMapTitle();
  requestMapRender();
}
function closeMapOverlay() {
  mapOverlay.hidden = true;
  mapPicker.hidden = true;
}
mapBtn.addEventListener("click", openMapOverlay);
document.getElementById("map-close").addEventListener("click", closeMapOverlay);
mapFollowBtn.addEventListener("click", () => {
  if (mapBrowse) exitMapBrowse();
  else setMapFollow(!mapFollow);
});
mapStopBtn.addEventListener("click", () => sendCommand(".go2 stop"));
setMapFollow(true);

// ---- Location picker: browse any mapped location ---------------------------

mapTitleEl.addEventListener("click", () => {
  if (!mapPicker.hidden) {
    mapPicker.hidden = true;
    return;
  }
  mapPicker.hidden = false;
  mapPickerFilter.value = "";
  mapPickerStatus.hidden = true;
  if (mapLocations.length) {
    renderMapPickerList();
  } else {
    mapPickerList.replaceChildren();
    mapPickerStatus.textContent = "Loading locations…";
    mapPickerStatus.hidden = false;
    mapPendingRequest = ++mapRequestCounter;
    sendJson("map_locations", { request_id: mapPendingRequest });
  }
});

mapPickerFilter.addEventListener("input", renderMapPickerList);

function renderMapPickerList() {
  const needle = mapPickerFilter.value.trim().toLowerCase();
  const shown = mapLocations.filter((l) => !needle || l.toLowerCase().includes(needle));
  mapPickerList.replaceChildren();
  for (const location of shown.slice(0, 200)) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.textContent = location;
    btn.addEventListener("click", () => {
      mapPickerStatus.textContent = `Loading ${location}…`;
      mapPickerStatus.hidden = false;
      mapPendingRequest = ++mapRequestCounter;
      sendJson("map_view", { request_id: mapPendingRequest, location });
    });
    mapPickerList.appendChild(btn);
  }
}

function handleMapLocations(d) {
  if (d.request_id !== mapPendingRequest) return;
  mapLocations = d.locations || [];
  mapPickerStatus.hidden = true;
  renderMapPickerList();
}

function handleMapBrowse(d) {
  if (d.request_id !== mapPendingRequest) return;
  if (d.error || !d.scene) {
    mapPickerStatus.textContent = d.error || "No map for that location";
    mapPickerStatus.hidden = false;
    return;
  }
  mapBrowse = { location: d.location, scene: d.scene };
  mapPicker.hidden = true;
  setMapFollow(false);
  // Center on the browsed layout.
  const rooms = d.scene.rooms;
  if (rooms.length) {
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (const r of rooms) {
      minX = Math.min(minX, r.x); maxX = Math.max(maxX, r.x);
      minY = Math.min(minY, r.y); maxY = Math.max(maxY, r.y);
    }
    mapCam.x = (minX + maxX) / 2;
    mapCam.y = (minY + maxY) / 2;
  }
  renderMapTitle();
  requestMapRender();
}

function resizeMapCanvas() {
  const rect = mapCanvas.parentElement.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  mapCanvas.width = Math.max(1, Math.round(rect.width * dpr));
  mapCanvas.height = Math.max(1, Math.round(rect.height * dpr));
}
window.addEventListener("resize", () => {
  if (!mapOverlay.hidden) {
    resizeMapCanvas();
    requestMapRender();
  }
});

function requestMapRender() {
  if (mapOverlay.hidden || mapRaf) return;
  mapRaf = requestAnimationFrame(() => {
    mapRaf = 0;
    renderMap();
  });
}

function mapCss(name, fallback) {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name);
  return v ? v.trim() : fallback;
}

function renderMap() {
  const ctx = mapCanvas.getContext("2d");
  const dpr = window.devicePixelRatio || 1;
  const w = mapCanvas.width / dpr;
  const h = mapCanvas.height / dpr;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  const scene = activeMapScene();
  const hasRooms = scene && scene.rooms.length > 0;
  mapEmptyEl.hidden = hasRooms;
  if (!hasRooms) return;

  const fg = mapCss("--fg", "#d6d6d6");
  const dim = mapCss("--fg-dim", "#8a8f98");
  const panel = mapCss("--bg", "#111318");
  const gold = mapCss("--st", "#d9b44f");
  const ppc = mapCam.ppc;
  const sx = (cx) => w / 2 + (cx - mapCam.x) * ppc;
  const sy = (cy) => h / 2 + (cy - mapCam.y) * ppc;
  const roomSize = Math.min(26, Math.max(3, ppc * 0.55));
  const showIds = ppc >= 20;
  const showLabels = ppc >= 14;
  const onScreen = (x, y) =>
    x > -ppc * 2 && x < w + ppc * 2 && y > -ppc * 2 && y < h + ppc * 2;

  // Edges under rooms.
  ctx.lineWidth = 1.2;
  for (const e of scene.edges) {
    const x1 = sx(e.x1), y1 = sy(e.y1), x2 = sx(e.x2), y2 = sy(e.y2);
    if (!onScreen(x1, y1) && !onScreen(x2, y2)) continue;
    if (e.k === 0) {
      ctx.strokeStyle = dim;
      ctx.setLineDash([]);
      ctx.beginPath();
      ctx.moveTo(x1, y1);
      ctx.lineTo(x2, y2);
      ctx.stroke();
    } else if (e.k === 1) {
      ctx.strokeStyle = dim;
      ctx.globalAlpha = 0.7;
      ctx.setLineDash([ppc * 0.25, ppc * 0.18]);
      ctx.beginPath();
      ctx.moveTo(x1, y1);
      ctx.lineTo(x2, y2);
      ctx.stroke();
      ctx.globalAlpha = 1;
      if (showLabels && e.l && Math.max(Math.abs(x2 - x1), Math.abs(y2 - y1)) >= ppc * 1.9) {
        ctx.setLineDash([]);
        ctx.fillStyle = dim;
        ctx.font = `${Math.min(13, Math.max(8, ppc * 0.45))}px sans-serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(e.l, (x1 + x2) / 2, (y1 + y2) / 2);
      }
    } else {
      // Stub: short dashed arrows at both ends toward the partner.
      const dx = x2 - x1, dy = y2 - y1;
      const len = Math.hypot(dx, dy) || 1;
      const ux = dx / len, uy = dy / len;
      const stub = ppc * 0.9;
      ctx.strokeStyle = dim;
      ctx.globalAlpha = 0.7;
      ctx.setLineDash([ppc * 0.18, ppc * 0.12]);
      for (const [ox, oy, tx, ty, partner] of [
        [x1, y1, ux, uy, e.br],
        [x2, y2, -ux, -uy, e.ar],
      ]) {
        ctx.beginPath();
        ctx.moveTo(ox, oy);
        ctx.lineTo(ox + tx * stub, oy + ty * stub);
        ctx.stroke();
        if (showIds && partner != null) {
          ctx.setLineDash([]);
          ctx.fillStyle = dim;
          ctx.font = `${Math.min(12, Math.max(7, ppc * 0.45))}px sans-serif`;
          ctx.textAlign = "center";
          ctx.textBaseline = "middle";
          ctx.fillText(String(partner), ox + tx * (stub + 8), oy + ty * (stub + 8));
          ctx.setLineDash([ppc * 0.18, ppc * 0.12]);
        }
      }
      ctx.globalAlpha = 1;
    }
  }
  ctx.setLineDash([]);

  // Group labels.
  if (showLabels) {
    ctx.fillStyle = dim;
    ctx.font = `${Math.min(14, Math.max(9, ppc * 0.5))}px sans-serif`;
    ctx.textAlign = "left";
    ctx.textBaseline = "bottom";
    for (const l of scene.labels) {
      const x = sx(l.x), y = sy(l.y);
      if (!onScreen(x, y)) continue;
      ctx.fillText(l.t, x - roomSize / 2, y - roomSize);
    }
  }

  // Rooms.
  for (const r of scene.rooms) {
    const x = sx(r.x), y = sy(r.y);
    if (!onScreen(x, y)) continue;
    const half = roomSize / 2;
    const isCurrent = !mapBrowse && !mapState.in_ghost && mapState.room === r.i;
    if (isCurrent) {
      ctx.fillStyle = gold;
      ctx.globalAlpha = 0.35;
      ctx.fillRect(x - half - 3, y - half - 3, roomSize + 6, roomSize + 6);
      ctx.globalAlpha = 1;
      ctx.strokeStyle = fg;
      ctx.lineWidth = 2;
      ctx.strokeRect(x - half - 3, y - half - 3, roomSize + 6, roomSize + 6);
      ctx.lineWidth = 1.2;
    }
    ctx.fillStyle = panel;
    ctx.strokeStyle = isCurrent ? fg : dim;
    ctx.fillRect(x - half, y - half, roomSize, roomSize);
    ctx.strokeRect(x - half, y - half, roomSize, roomSize);
    if (r.e) {
      ctx.fillStyle = gold;
      ctx.beginPath();
      ctx.arc(x, y - half, Math.min(4, Math.max(1.5, roomSize * 0.18)), 0, Math.PI * 2);
      ctx.fill();
    }
    if (showIds) {
      ctx.fillStyle = dim;
      ctx.font = `${Math.min(11, Math.max(8, ppc * 0.32))}px sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      ctx.fillText(String(r.i), x, y + half + 2);
    }
    if (isCurrent) drawMapExitTicks(ctx, x, y, half, fg);
  }

  // Ghost sketches: dashed and dim, unmistakable from mapped truth.
  // Live view only - a browsed location has no session sketches.
  if (!mapBrowse && mapState.ghosts && mapState.ghosts.length) {
    ctx.globalAlpha = 0.65;
    ctx.strokeStyle = dim;
    ctx.setLineDash([Math.max(2, ppc * 0.12), Math.max(1.5, ppc * 0.08)]);
    for (const e of mapState.ghost_edges || []) {
      ctx.beginPath();
      ctx.moveTo(sx(e.x1), sy(e.y1));
      ctx.lineTo(sx(e.x2), sy(e.y2));
      ctx.stroke();
      if (showLabels && e.l) {
        ctx.fillStyle = dim;
        ctx.font = `${Math.min(13, Math.max(8, ppc * 0.45))}px sans-serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(e.l, (sx(e.x1) + sx(e.x2)) / 2, (sy(e.y1) + sy(e.y2)) / 2);
      }
    }
    for (const g of mapState.ghosts) {
      const x = sx(g.x), y = sy(g.y);
      const half = roomSize / 2;
      if (g.cur) {
        ctx.globalAlpha = 1;
        ctx.strokeStyle = fg;
        ctx.lineWidth = 2;
        ctx.strokeRect(x - half - 3, y - half - 3, roomSize + 6, roomSize + 6);
        ctx.lineWidth = 1.2;
        ctx.strokeStyle = dim;
        ctx.globalAlpha = 0.65;
      }
      ctx.strokeRect(x - half, y - half, roomSize, roomSize);
      if (g.cur) drawMapExitTicks(ctx, x, y, half, fg);
    }
    ctx.setLineDash([]);
    ctx.globalAlpha = 1;
  }

  // Travel progress banner while .go2 walks (live view only).
  if (mapState.travel && !mapBrowse) {
    const t = mapState.travel;
    const label = `→ ${t.dest} · ${t.done}/${t.total} rooms · ETA ${t.eta}`;
    ctx.font = "12px sans-serif";
    const pad = 6;
    const tw = ctx.measureText(label).width;
    ctx.fillStyle = panel;
    ctx.globalAlpha = 0.85;
    ctx.fillRect(4, 4, tw + pad * 2, 22);
    ctx.globalAlpha = 1;
    ctx.strokeStyle = dim;
    ctx.strokeRect(4, 4, tw + pad * 2, 22);
    ctx.fillStyle = fg;
    ctx.textAlign = "left";
    ctx.textBaseline = "middle";
    ctx.fillText(label, 4 + pad, 15);
  }
}

// Accent ticks for the current room's exits, reusing the compass state the
// room widget already maintains.
const MAP_TICK_DIRS = {
  n: [0, -1], s: [0, 1], e: [1, 0], w: [-1, 0],
  ne: [0.707, -0.707], nw: [-0.707, -0.707], se: [0.707, 0.707], sw: [-0.707, 0.707],
};
function drawMapExitTicks(ctx, x, y, half, color) {
  ctx.strokeStyle = color;
  ctx.lineWidth = 2;
  const len = Math.min(8, Math.max(3, half * 0.8));
  for (const exit of roomExits) {
    const key = String(exit).toLowerCase().trim();
    const dir = MAP_TICK_DIRS[EXIT_ALIASES[key] || key];
    if (!dir) continue;
    const fx = x + dir[0] * (half + 2);
    const fy = y + dir[1] * (half + 2);
    ctx.beginPath();
    ctx.moveTo(fx, fy);
    ctx.lineTo(fx + dir[0] * len, fy + dir[1] * len);
    ctx.stroke();
  }
  ctx.lineWidth = 1.2;
}

// ---- Map touch: drag pans, pinch zooms, tap walks --------------------------

const mapPointers = new Map();
let mapMoved = false;
let mapPinchStart = 0;

mapCanvas.addEventListener("pointerdown", (ev) => {
  // Capture can fail (pointer already gone); the tap must still register.
  try { mapCanvas.setPointerCapture(ev.pointerId); } catch { /* fine */ }
  mapPointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
  if (mapPointers.size === 1) mapMoved = false;
  if (mapPointers.size === 2) {
    const [a, b] = [...mapPointers.values()];
    mapPinchStart = Math.hypot(a.x - b.x, a.y - b.y) / mapCam.ppc;
  }
});

mapCanvas.addEventListener("pointermove", (ev) => {
  const prev = mapPointers.get(ev.pointerId);
  if (!prev) return;
  const cur = { x: ev.clientX, y: ev.clientY };
  if (mapPointers.size === 1) {
    const dx = cur.x - prev.x;
    const dy = cur.y - prev.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) {
      mapMoved = true;
      setMapFollow(false);
      mapCam.x -= dx / mapCam.ppc;
      mapCam.y -= dy / mapCam.ppc;
      requestMapRender();
    }
  }
  mapPointers.set(ev.pointerId, cur);
  if (mapPointers.size === 2 && mapPinchStart > 0) {
    mapMoved = true;
    const [a, b] = [...mapPointers.values()];
    const dist = Math.hypot(a.x - b.x, a.y - b.y);
    mapCam.ppc = Math.min(64, Math.max(4, dist / mapPinchStart));
    requestMapRender();
  }
});

function mapPointerEnd(ev) {
  const wasTap = mapPointers.size === 1 && !mapMoved;
  mapPointers.delete(ev.pointerId);
  if (mapPointers.size < 2) mapPinchStart = 0;
  if (!wasTap || ev.type !== "pointerup") return;
  const scene = activeMapScene();
  if (!scene) return;
  // Tap: walk to the nearest room within a finger of the touch point.
  const rect = mapCanvas.getBoundingClientRect();
  const px = ev.clientX - rect.left;
  const py = ev.clientY - rect.top;
  const cx = mapCam.x + (px - rect.width / 2) / mapCam.ppc;
  const cy = mapCam.y + (py - rect.height / 2) / mapCam.ppc;
  let best = null;
  let bestDist = Infinity;
  for (const r of scene.rooms) {
    const d = Math.hypot(r.x - cx, r.y - cy);
    if (d < bestDist) {
      bestDist = d;
      best = r;
    }
  }
  const slopCells = Math.max(0.55, 14 / mapCam.ppc);
  if (best && bestDist <= slopCells) {
    sendCommand(`.go2 ${best.i}`);
    mapOverlay.hidden = true;
  }
}
mapCanvas.addEventListener("pointerup", mapPointerEnd);
mapCanvas.addEventListener("pointercancel", mapPointerEnd);

mapCanvas.addEventListener("wheel", (ev) => {
  ev.preventDefault();
  const factor = ev.deltaY < 0 ? 1.15 : 1 / 1.15;
  mapCam.ppc = Math.min(64, Math.max(4, mapCam.ppc * factor));
  requestMapRender();
}, { passive: false });
