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
const roomNameEl = document.getElementById("room-name");
const connEl = document.getElementById("conn");
const rtFill = document.getElementById("rt-fill");
const rtLabel = document.getElementById("rt-label");
const handLeftEl = document.getElementById("hand-left");
const handRightEl = document.getElementById("hand-right");
const indicatorsEl = document.getElementById("indicators");

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

function serverNow() {
  return Date.now() / 1000 + state.clockOffset;
}

function setConnected(up) {
  connEl.textContent = up ? "live" : "reconnecting…";
  connEl.className = "conn " + (up ? "conn-up" : "conn-down");
}

// ---- Per-stream buffers and chips ---------------------------------------

const buffers = new Map(); // stream -> { lines: [], unread: 0, chip, badge }
let activeStream = "main";

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
  chip.addEventListener("click", () => setActiveStream(stream));
  // Keep Story first, everything else in arrival order.
  chipsBar.appendChild(chip);
  buf = { lines: [], unread: 0, chip, badge };
  buffers.set(stream, buf);
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
  roomNameEl.replaceChildren();
  const name = document.createElement("span");
  name.textContent = room.name || "—";
  roomNameEl.appendChild(name);
  for (const exit of room.exits || []) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "exit";
    btn.textContent = exit;
    btn.addEventListener("click", () => sendCommand(exit));
    roomNameEl.appendChild(btn);
  }
}

function setHands(d) {
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

function setRt(rt) {
  if (typeof rt.server_time === "number" && rt.server_time > 0) {
    state.clockOffset = rt.server_time - Date.now() / 1000;
  }
  state.rtEnd = rt.roundtime_end ?? null;
  state.ctEnd = rt.casttime_end ?? null;
  const end = Math.max(state.rtEnd ?? 0, state.ctEnd ?? 0);
  state.rtTotal = end > serverNow() ? end - serverNow() : 0;
}

function tickRt() {
  const end = Math.max(state.rtEnd ?? 0, state.ctEnd ?? 0);
  const remaining = end - serverNow();
  const isCast = (state.ctEnd ?? 0) >= (state.rtEnd ?? 0) && (state.ctEnd ?? 0) > 0;
  if (remaining > 0) {
    const frac = state.rtTotal > 0 ? Math.min(1, remaining / state.rtTotal) : 0;
    rtFill.style.width = `${frac * 100}%`;
    rtFill.style.background = isCast ? "var(--ct)" : "var(--rt)";
    rtLabel.textContent = `${isCast ? "CT" : "RT"} ${Math.ceil(remaining)}`;
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
  setVitals(d.vitals);
  setRoom(d.room);
  setHands(d.hands || {});
  setIndicators(d.indicators || {});
  setRt(d.rt);
  if (autoScroll) scrollToBottom();
}

function handleMessage(msg) {
  switch (msg.t) {
    case "hello":
      if (msg.d.character) document.title = `${msg.d.character} — VellumFE`;
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
    case "rt": setRt(msg.d); break;
    case "menu": handleMenu(msg.d); break;
    default:
      console.debug("unknown message type", msg.t);
  }
}

function connect() {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  const ws = new WebSocket(`${proto}//${location.host}/ws`);
  state.ws = ws;

  ws.onopen = () => {
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
    setTimeout(connect, state.reconnectDelay);
    state.reconnectDelay = Math.min(state.reconnectDelay * 2, 10000);
  };
  ws.onerror = () => ws.close();
}

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
  clearTimeout(sheetTimeout);
}

function openSheet(title) {
  sheetTitle.textContent = title;
  sheetItems.replaceChildren();
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
    onPick();
    closeSheet();
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
  if (ev.target.closest("span.link")) return;
  if (ev.target.closest("#repeat-btn")) return; // long-press opens history
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

// ---- Command input, repeat, history ---------------------------------------

const inputForm = document.getElementById("input-row");
const cmdInput = document.getElementById("cmd-input");
const repeatBtn = document.getElementById("repeat-btn");

let history = [];
try {
  history = JSON.parse(localStorage.getItem(HISTORY_KEY) || "[]");
} catch { /* corrupted storage — start fresh */ }

function recordHistory(text) {
  if (history[0] === text) return;
  history.unshift(text);
  if (history.length > HISTORY_MAX) history.length = HISTORY_MAX;
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(history));
  } catch { /* storage full/blocked — history just won't persist */ }
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

// Repeat button: tap = resend last command; hold = history sheet.
let repeatHold = null;
let repeatHeld = false;
repeatBtn.addEventListener("pointerdown", () => {
  repeatHeld = false;
  repeatHold = setTimeout(() => {
    repeatHeld = true;
    openSheet("History");
    if (!history.length) {
      sheetNote("No commands yet", true);
      return;
    }
    for (const cmd of history) {
      sheetButton(cmd, () => {
        sendCommand(cmd);
        recordHistory(cmd);
      });
    }
  }, 450);
});
repeatBtn.addEventListener("pointerup", () => clearTimeout(repeatHold));
repeatBtn.addEventListener("pointerleave", () => clearTimeout(repeatHold));
repeatBtn.addEventListener("click", () => {
  if (repeatHeld) return; // the hold already opened the history sheet
  if (history[0]) sendCommand(history[0]);
});

// Hardware keyboard: up/down arrows browse history in the input field.
let historyIndex = -1;
cmdInput.addEventListener("keydown", (ev) => {
  if (ev.key === "ArrowUp") {
    if (historyIndex < history.length - 1) historyIndex += 1;
    if (history[historyIndex]) cmdInput.value = history[historyIndex];
    ev.preventDefault();
  } else if (ev.key === "ArrowDown") {
    historyIndex -= 1;
    if (historyIndex < 0) {
      historyIndex = -1;
      cmdInput.value = "";
    } else {
      cmdInput.value = history[historyIndex] || "";
    }
    ev.preventDefault();
  } else {
    historyIndex = -1;
  }
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
