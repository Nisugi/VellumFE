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
  roomId = room.id || null;
  roomExits = room.exits || [];
  renderTitle();
  renderCompass();
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
  setCharInfo(d.char_info || {});
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
    case "highlights": handleHighlightsReply(msg.d); break;
    case "colors": handleColorsReply(msg.d); break;
    case "injuries": setInjuries(msg.d); break;
    case "targets": setTargets(msg.d); break;
    case "charinfo": setCharInfo(msg.d); break;
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
  session = d || { state: "connected", session_control: false };
  if (session.character) setCharacter(session.character);
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

// ---- Login mode toggle (play.net direct vs Lich attach) --------------------

const modeDirectBtn = document.getElementById("mode-direct");
const modeLichBtn = document.getElementById("mode-lich");
const directFields = document.getElementById("direct-fields");
const lichFields = document.getElementById("lich-fields");
const lichHostInput = document.getElementById("lich-host");
const lichPortInput = document.getElementById("lich-port");
const lichNameInput = document.getElementById("lich-name");
const lichWarning = document.getElementById("lich-warning");
let loginMode = "direct";

function setLoginMode(mode) {
  loginMode = mode;
  modeDirectBtn.classList.toggle("mode-active", mode === "direct");
  modeDirectBtn.setAttribute("aria-selected", String(mode === "direct"));
  modeLichBtn.classList.toggle("mode-active", mode === "lich");
  modeLichBtn.setAttribute("aria-selected", String(mode === "lich"));
  directFields.hidden = mode !== "direct";
  lichFields.hidden = mode !== "lich";
}
modeDirectBtn.addEventListener("click", () => setLoginMode("direct"));
modeLichBtn.addEventListener("click", () => setLoginMode("lich"));

// Deep-linked Lich target: open the Lich tab prefilled and let the user
// press Connect.
if (bootLich) {
  lichHostInput.value = bootLich.host;
  lichPortInput.value = bootLich.port;
  lichNameInput.value = bootLich.name;
  lichHostInput.dispatchEvent(new Event("input"));
  setLoginMode("lich");
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

sessionForm.addEventListener("submit", (ev) => {
  ev.preventDefault();
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
// classes. Both persist per device — nothing crosses the wire.

const UI_PREFS_KEY = "vellum-ui-prefs";
let uiPrefs = { theme: "dark", hide: {} };
try {
  const stored = JSON.parse(localStorage.getItem(UI_PREFS_KEY) || "{}");
  if (stored && typeof stored === "object") {
    uiPrefs = { theme: stored.theme || "dark", hide: stored.hide || {} };
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
];

const CHROME_TOGGLES = [
  ["macrorail", "Macro bar (bottom)"],
  ["compass", "Compass"],
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
  sheetButton(
    soundMuted ? "Sound alerts: off — tap to enable" : "Sound alerts: on — tap to mute",
    () => {
      soundMuted = !soundMuted;
      try {
        localStorage.setItem(SOUND_MUTE_KEY, soundMuted ? "1" : "");
      } catch { /* private mode */ }
    },
  );
  sheetButton("Highlight rules (this profile)", () => openHighlightList("profile"));
  sheetButton("Highlight rules (global)", () => openHighlightList("global"));
  sheetButton("Colors (this profile)", () => openColorsEditor("profile"));
  sheetButton("Colors (global)", () => openColorsEditor("global"));
  for (const file of EDITOR_FILES) {
    sheetButton(file.label, () => openConfigEditor(file));
  }
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
  // Merge the form over the existing rule so fields the form doesn't
  // cover survive the round trip.
  const base = (hlEditingName && hlRules[hlEditingName]) || {};
  const rule = { ...base, pattern };
  const sound = document.getElementById("hl-sound").value;
  if (hlColors.fg) rule.fg = hlColors.fg; else delete rule.fg;
  if (hlColors.bg) rule.bg = hlColors.bg; else delete rule.bg;
  if (sound) rule.sound = sound; else delete rule.sound;
  rule.bold = document.getElementById("hl-bold").checked;
  rule.color_entire_line = document.getElementById("hl-line").checked;

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
// the input bar (iOS Safari doesn't resize the layout viewport). iOS also
// *scrolls* the page to reveal the focused input instead of resizing, so
// --vvt follows the viewport's offset to keep the app under the visible
// region (stays 0 on platforms that resize).
const vv = window.visualViewport;
function syncViewport() {
  if (!vv) return;
  document.documentElement.style.setProperty("--vvh", `${vv.height}px`);
  document.documentElement.style.setProperty("--vvt", `${vv.offsetTop}px`);
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

  // Doll
  const doll = document.createElement("div");
  doll.id = "status-doll";
  const shapes = DOLL_PARTS.map(([id, shape]) => {
    const level = injuries[id] || 0;
    const fill = LEVEL_COLORS[level] || "var(--border)";
    return shape.replace("/>", ` fill="${fill}"/>`);
  }).join("");
  doll.innerHTML =
    `<svg viewBox="0 0 140 132" role="img" aria-label="Injury doll">${shapes}</svg>`;
  panel.appendChild(doll);

  // Back / nervous system indicator chips (no geometry on the doll).
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

  const prompts = doc.prompt_colors || [];
  if (prompts.length) {
    colorsList.appendChild(colorsSection("Prompt characters"));
    for (const p of prompts) {
      colorsList.appendChild(colorRow('"' + p.character + '"', p, "fg", "bg"));
    }
  }

  if (!names.length && !prompts.length) {
    colorsList.appendChild(Object.assign(document.createElement("p"), {
      className: "hl-empty",
      textContent: "No presets or prompt colors in this file.",
    }));
  }

  const note = document.createElement("p");
  note.className = "hl-empty";
  note.textContent =
    "Other sections (TUI chrome, spell colors, palette) are preserved as-is; edit them under Advanced.";
  colorsList.appendChild(note);
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
