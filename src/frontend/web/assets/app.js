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
  setRt(d.rt);
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
    case "rt": setRt(msg.d); break;
    case "menu": handleMenu(msg.d); break;
    case "macros": macros = msg.d; renderMacros(); break;
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
  for (const btn of group.buttons) {
    const el = document.createElement("button");
    el.type = "button";
    el.className = "macro-btn";
    el.textContent = btn.options && btn.options.length ? `${btn.label} ›` : btn.label;
    if (btn.color) {
      el.style.background = "none";
      el.style.border = `1px solid ${btn.color}`;
      el.style.color = btn.color;
    }
    el.addEventListener("click", () => activateMacro(btn));
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
      if (btn.editable && !(btn.options && btn.options.length)) {
        list.push({ group: group.name, btn });
      }
    }
  }
  for (const btn of macros.floating) {
    if (btn.editable && !(btn.options && btn.options.length)) {
      list.push({ group: null, btn });
    }
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
  cmdWrap.append("Command", cmdInputEl);

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

  form.append(labelWrap, cmdWrap, placeWrap, colorRow, confirmRow, actions);
  form.addEventListener("submit", (ev) => {
    ev.preventDefault();
    const label = labelInput.value.trim();
    const command = cmdInputEl.value.trim();
    if (!label || !command) return;
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
