// VellumFE web client v0 — read-only viewer.
// Protocol: JSON envelopes { v, seq, t, d } over /ws. See
// docs/mobile-web-frontend-plan.md and src/frontend/web/protocol.rs.

const MAX_LINES = 2000;
// v0 shows the main story stream; other streams arrive but are skipped.
const VISIBLE_STREAMS = new Set(["main"]);

const pane = document.getElementById("text-pane");
const roomNameEl = document.getElementById("room-name");
const connEl = document.getElementById("conn");
const rtFill = document.getElementById("rt-fill");
const rtLabel = document.getElementById("rt-label");

const state = {
  lastSeq: 0,          // highest text seq rendered (the resume cursor)
  session: null,       // server process id; seqs restart when it changes
  ws: null,
  clockOffset: 0,      // serverTime - localTime, seconds
  rtEnd: null,         // roundtime end, server seconds
  ctEnd: null,         // casttime end, server seconds
  rtTotal: 0,          // duration of current RT for the progress bar
  reconnectDelay: 1000,
};

function serverNow() {
  return Date.now() / 1000 + state.clockOffset;
}

function setConnected(up) {
  connEl.textContent = up ? "live" : "reconnecting…";
  connEl.className = "conn " + (up ? "conn-up" : "conn-down");
}

function atBottom() {
  return pane.scrollTop + pane.clientHeight >= pane.scrollHeight - 40;
}

function scrollToBottom() {
  pane.scrollTop = pane.scrollHeight;
}

// Autoscroll stickiness is an explicit flag updated on scroll events, not
// re-measured per append: measuring mid-flood reads a stale layout and
// unsticks. The user scrolling up disables it; returning to the bottom
// (or a snapshot reset) re-enables it.
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
    // Tappable noun: a real game object (not a <d> direct tag).
    const link = seg.link_data;
    if (link && link.exist_id && link.exist_id !== "_direct_") {
      span.classList.add("link");
      span.dataset.existId = link.exist_id;
      span.dataset.noun = link.noun;
    }
    div.appendChild(span);
  }
  return div;
}

// Incoming lines are queued and rendered once per animation frame as a
// single fragment. Per-line appends force two layout flushes each, which
// floods the main thread when output scrolls fast (e.g. held-down LOOK)
// and breaks autoscroll.
const pendingLines = [];
let renderScheduled = false;

function flushPendingLines() {
  renderScheduled = false;
  if (!pendingLines.length) return;
  const frag = document.createDocumentFragment();
  for (const line of pendingLines) frag.appendChild(renderLine(line));
  pendingLines.length = 0;
  pane.appendChild(frag);
  while (pane.childElementCount > MAX_LINES) pane.firstChild.remove();
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
  if (!VISIBLE_STREAMS.has(stream)) return;
  pendingLines.push(line);
  scheduleRender();
}

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

function appendMarker(text) {
  const div = document.createElement("div");
  div.className = "line marker";
  div.textContent = text;
  pane.appendChild(div);
}

function handleSnapshot(d) {
  // mode: "full" = fresh view; "resume" = only lines newer than our
  // cursor (keep the pane); "gap" = lines were evicted before we could
  // resume (keep the pane, mark the hole).
  if (d.mode === "full") {
    pane.replaceChildren();
    state.lastSeq = 0;
    pendingLines.length = 0;
    autoScroll = true;
  } else if (d.mode === "gap") {
    appendMarker("— missed output —");
  }
  for (const item of d.text) appendText(item.seq, item.stream, item.line);
  flushPendingLines();
  setVitals(d.vitals);
  setRoom(d.room);
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
    case "rt": setRt(msg.d); break;
    case "menu": handleMenu(msg.d); break;
    case "hands":
    case "indicators":
      break; // rendered in a later phase
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

// ---- Noun-tap bottom-sheet menu ----------------------------------------
// Tapping a noun sends link_tap; the server issues `_menu` upstream and
// the response comes back (to this client only) as a `menu` message. A
// pick just sends the item's ready-made game command via `cmd`.

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

function openSheetLoading(noun) {
  sheetTitle.textContent = noun;
  sheetItems.replaceChildren();
  const loading = document.createElement("div");
  loading.className = "sheet-empty";
  loading.textContent = "…";
  sheetItems.appendChild(loading);
  sheet.hidden = false;
  sheetBackdrop.hidden = false;
  // Never leave the sheet spinning if the response is lost (disconnect,
  // stale request id, server restart).
  clearTimeout(sheetTimeout);
  sheetTimeout = setTimeout(() => {
    if (!sheet.hidden && pendingMenuRequest !== null) {
      sheetItems.replaceChildren();
      const empty = document.createElement("div");
      empty.className = "sheet-empty";
      empty.textContent = "No response — tap to dismiss";
      empty.addEventListener("click", closeSheet);
      sheetItems.appendChild(empty);
    }
  }, 5000);
}

sheetBackdrop.addEventListener("click", closeSheet);

pane.addEventListener("click", (ev) => {
  const span = ev.target.closest("span.link");
  if (!span || !state.ws || state.ws.readyState !== WebSocket.OPEN) return;
  const requestId = ++menuRequestCounter;
  pendingMenuRequest = requestId;
  openSheetLoading(span.dataset.noun || span.textContent);
  state.ws.send(JSON.stringify({
    t: "link_tap",
    d: {
      request_id: requestId,
      exist_id: span.dataset.existId,
      noun: span.dataset.noun || "",
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
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "sheet-item";
    btn.textContent = item.text;
    btn.addEventListener("click", () => {
      sendCommand(item.command);
      closeSheet();
    });
    sheetItems.appendChild(btn);
    rendered += 1;
  }
  if (rendered === 0) {
    const empty = document.createElement("div");
    empty.className = "sheet-empty";
    empty.textContent = "No actions available";
    sheetItems.appendChild(empty);
  }
}

// Command input: sends through the same core path as locally typed
// commands. The echo comes back over the text stream, so we don't render
// anything locally.
const inputForm = document.getElementById("input-row");
const cmdInput = document.getElementById("cmd-input");
inputForm.addEventListener("submit", (ev) => {
  ev.preventDefault();
  const text = cmdInput.value.trim();
  if (!text) return;
  sendCommand(text);
  cmdInput.value = "";
});

setInterval(tickRt, 100);
connect();
