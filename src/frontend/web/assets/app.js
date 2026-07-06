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
  lastSeq: 0,          // highest text seq rendered (dedupe across snapshot/deltas)
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
    div.appendChild(span);
  }
  return div;
}

function appendText(seq, stream, line) {
  if (seq <= state.lastSeq) return; // duplicate (snapshot/delta overlap)
  state.lastSeq = seq;
  if (!VISIBLE_STREAMS.has(stream)) return;
  const stick = atBottom();
  pane.appendChild(renderLine(line));
  while (pane.childElementCount > MAX_LINES) pane.firstChild.remove();
  if (stick) scrollToBottom();
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

function setRoom(room) {
  const exits = room.exits && room.exits.length ? `  [${room.exits.join(", ")}]` : "";
  roomNameEl.textContent = (room.name || "—") + exits;
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

function handleSnapshot(d) {
  pane.replaceChildren();
  // A snapshot resets the text pane; render its scrollback from scratch.
  state.lastSeq = 0;
  for (const item of d.text) appendText(item.seq, item.stream, item.line);
  setVitals(d.vitals);
  setRoom(d.room);
  setRt(d.rt);
  scrollToBottom();
}

function handleMessage(msg) {
  switch (msg.t) {
    case "hello":
      if (msg.d.character) document.title = `${msg.d.character} — VellumFE`;
      break;
    case "snapshot": handleSnapshot(msg.d); break;
    case "text": appendText(msg.seq, msg.d.stream, msg.d.line); break;
    case "vitals": setVitals(msg.d); break;
    case "room": setRoom(msg.d); break;
    case "rt": setRt(msg.d); break;
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

setInterval(tickRt, 100);
connect();
