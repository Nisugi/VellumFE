import DesktopSession, {
  characterTitleText,
  VELLUM_TOKEN_STORAGE_KEY,
  shouldShowVellumIdle,
} from "./session.js";
import DesktopInteractionCoordinator from "./interactions.js";
import { DesktopMapViewport } from "./map.js";
import { DesktopWorkspace } from "./workspace.js";
import { createDesktopWorkspaceStore } from "./workspace-persistence.js";
import {
  DEFAULT_FONT_SCALE,
  normalizeFontScale,
  readFontScale,
  writeFontScale,
} from "./font-scale.js";

const MAX_MOUNTED_LINES = 1000;
const EXIT_ALIASES = Object.freeze({
  north: "n",
  northeast: "ne",
  east: "e",
  southeast: "se",
  south: "s",
  southwest: "sw",
  west: "w",
  northwest: "nw",
  up: "up",
  down: "down",
  out: "out",
});

const root = document.getElementById("desktop-app");
const connectionStatus = document.getElementById("connection-status");
const characterTitle = document.getElementById("character-title");
const commandForm = document.getElementById("command-form");
const commandInput = document.getElementById("command-input");
const commandButton = commandForm.querySelector("button[type='submit']");
const commandStatus = document.getElementById("command-status");
const sessionExitButton = document.getElementById("session-exit-button");
const vellumIdle = document.getElementById("vellum-idle");
const vellumIdleTitle = document.getElementById("vellum-idle-title");
const vellumIdleMessage = document.getElementById("vellum-idle-message");
const textPaneState = new WeakMap();
const commandHistory = [];
let commandHistoryIndex = 0;
let latestView = null;
let clockOffsetSeconds = 0;
let interaction = null;
let menuAnchor = null;
let menuTimeout = null;
let currentFontScale = readFontScale(window.localStorage);

if (!root) throw new Error("Vellum Despana root is missing");
if (!sessionExitButton) throw new Error("Vellum Despana session exit control is missing");
if (!vellumIdle || !vellumIdleTitle || !vellumIdleMessage) {
  throw new Error("Vellum idle-session handoff is missing");
}
document.documentElement.style.setProperty("--font-scale", `${currentFontScale}%`);
function renderIdleSurface(view) {
  const visible = shouldShowVellumIdle(view);
  vellumIdle.hidden = !visible;
  root.inert = visible;
  if (!visible) return;

  const denied = view?.connection?.status === "denied";
  const waitingForTransport = !view?.session && ["connecting", "authenticating"]
    .includes(view?.connection?.status);
  vellumIdleTitle.textContent = denied
    ? "Pairing required"
    : waitingForTransport
      ? "Connecting to VellumFE"
      : "No active game session";
  vellumIdleMessage.textContent = denied
    ? "Return to the VellumFE Launcher to reconnect this presentation. You may also open Vellum's play page separately to enter a new pairing token."
    : waitingForTransport
      ? "Vellum Despana is waiting for VellumFE's authenticated session state."
      : "Start or attach the character from the VellumFE Launcher. Vellum Despana will appear automatically when that session is ready.";
}

function pairingToken() {
  // The URL hash must win: a fresh #token= arrives before DesktopSession has
  // persisted it to localStorage (that happens much later in this module), so
  // preferring storage here would send a stale token on the first catalog
  // fetch. Matches DesktopSession's own precedence (session.js).
  return new URLSearchParams(window.location.hash.replace(/^#/, "")).get("token") ||
    window.localStorage.getItem(VELLUM_TOKEN_STORAGE_KEY) ||
    "";
}

const workspace = new DesktopWorkspace(root, {
  storage: createDesktopWorkspaceStore({
    localStorage: window.localStorage,
    token: pairingToken,
  }),
  reportError(error) {
    console.error("Vellum Despana workspace error:", error);
  },
});
const mapController = createMapController(
  document.getElementById("map-canvas"),
  document.getElementById("map-empty"),
  {
    classicStage: document.getElementById("map-classic-stage"),
    classicImage: document.getElementById("map-classic-image"),
    classicMarker: document.getElementById("map-classic-marker"),
    selector: document.getElementById("map-selector"),
    classicButton: document.getElementById("map-mode-classic"),
    localButton: document.getElementById("map-mode-local"),
    fontScale: () => currentFontScale / 100,
    token: pairingToken,
    requestLocations(requestId) {
      return session.dispatch({ kind: "map-locations", requestId });
    },
    requestLocalMap(requestId, location) {
      return session.dispatch({ kind: "map-view", requestId, location });
    },
    travelToRoom(roomId) {
      return submitCommand(`.go2 ${roomId}`, `Traveling to room ${roomId}`);
    },
    reportRoom(roomId) {
      const text = String(roomId);
      commandStatus.textContent = `Map room ID: ${text}`;
      navigator.clipboard?.writeText(text).then(
        () => { commandStatus.textContent = `Map room ID copied: ${text}`; },
        () => {},
      );
    },
  },
);

const viewMenuButton = document.getElementById("view-menu-button");
const viewMenu = document.getElementById("view-menu");
const fontScaleInput = document.getElementById("font-scale");
const fontScaleValue = document.getElementById("font-scale-value");
const fontScaleReset = document.getElementById("font-scale-reset");
if (!viewMenuButton || !viewMenu || !fontScaleInput || !fontScaleValue || !fontScaleReset) {
  throw new Error("Font scale controls are missing");
}

function showFontScale(value, persist = false) {
  currentFontScale = persist
    ? writeFontScale(window.localStorage, value)
    : normalizeFontScale(value);
  document.documentElement.style.setProperty("--font-scale", `${currentFontScale}%`);
  fontScaleInput.value = String(currentFontScale);
  fontScaleValue.textContent = `${currentFontScale}%`;
  mapController.refreshTypography();
}

function setViewMenuOpen(open, focusControl = false) {
  viewMenu.hidden = !open;
  viewMenuButton.setAttribute("aria-expanded", String(open));
  if (open && focusControl) fontScaleInput.focus();
}

showFontScale(currentFontScale);
viewMenuButton.addEventListener("click", (event) => {
  event.preventDefault();
  setViewMenuOpen(viewMenu.hidden, true);
});
fontScaleInput.addEventListener("input", () => showFontScale(Number(fontScaleInput.value)));
fontScaleInput.addEventListener("change", () => showFontScale(Number(fontScaleInput.value), true));
fontScaleReset.addEventListener("click", () => showFontScale(DEFAULT_FONT_SCALE, true));
document.addEventListener("pointerdown", (event) => {
  if (viewMenu.hidden) return;
  if (viewMenu.contains(event.target) || viewMenuButton.contains(event.target)) return;
  setViewMenuOpen(false);
});
document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape" || viewMenu.hidden) return;
  event.preventDefault();
  setViewMenuOpen(false);
  viewMenuButton.focus();
});

function empty(container, message) {
  container.replaceChildren();
  const node = document.createElement("p");
  node.className = "empty-state";
  node.textContent = message;
  container.appendChild(node);
}

function styledSegment(segment) {
  const node = document.createElement("span");
  node.textContent = segment.text || "";
  if (segment.fg) node.style.color = segment.fg;
  if (segment.bg) node.style.backgroundColor = segment.bg;
  if (segment.bold) node.classList.add("text-bold");
  if (segment.mono) node.classList.add("text-mono");
  if (segment.span_type === "Monsterbold") node.classList.add("text-monster");
  if (segment.span_type === "PlayerTitle") node.classList.add("text-player-title");

  const link = segment.link_data;
  if (link?.exist_id) {
    node.classList.add("game-link");
    node.tabIndex = 0;
    node.setAttribute("role", "link");
    node.title = link.noun || link.text || "Game action";
    const activate = () => activateLink(link, node);
    node.addEventListener("click", activate);
    node.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      activate();
    });
  }
  return node;
}

function styledLine(line) {
  const node = document.createElement("div");
  node.className = "text-line";
  for (const segment of line?.segments || []) {
    if (segment.text) node.appendChild(styledSegment(segment));
  }
  return node;
}

function activateLink(link, anchor = null) {
  try {
    const effect = interaction.activate(link);
    if (effect.type === "pending-menu") {
      menuAnchor = anchor;
      showGameMenuLoading(link.noun || link.text || "Actions");
      commandStatus.textContent = `Loading actions for ${link.noun || link.text || "item"}`;
    } else if (effect.type === "url") {
      closeGameMenu();
      commandStatus.textContent = "Opened game link";
    } else if (effect.type === "dispatched") {
      closeGameMenu();
      commandStatus.textContent = `Sent (unconfirmed): ${link.text || link.noun}`;
    }
  } catch (error) {
    commandStatus.textContent = error?.message || "Action was not sent";
  }
}

const gameContextMenu = document.createElement("div");
gameContextMenu.className = "game-context-menu";
gameContextMenu.setAttribute("role", "menu");
gameContextMenu.setAttribute("aria-label", "Game actions");
gameContextMenu.hidden = true;
document.body.appendChild(gameContextMenu);

function positionGameMenu() {
  const rect = menuAnchor?.getBoundingClientRect?.();
  const left = Math.max(8, Math.min(
    window.innerWidth - gameContextMenu.offsetWidth - 8,
    rect?.left ?? 12,
  ));
  const top = Math.max(8, Math.min(
    window.innerHeight - gameContextMenu.offsetHeight - 8,
    rect?.bottom ?? 36,
  ));
  gameContextMenu.style.left = `${left}px`;
  gameContextMenu.style.top = `${top}px`;
}

function showGameMenuLoading(noun) {
  clearTimeout(menuTimeout);
  const label = document.createElement("div");
  label.className = "module-menu-heading";
  label.textContent = noun;
  const loading = document.createElement("div");
  loading.className = "empty-state";
  loading.textContent = "Loading actions…";
  gameContextMenu.setAttribute("aria-label", `${noun} actions`);
  gameContextMenu.replaceChildren(label, loading);
  gameContextMenu.hidden = false;
  requestAnimationFrame(positionGameMenu);
  menuTimeout = setTimeout(() => {
    if (!gameContextMenu.hidden) loading.textContent = "No response — click elsewhere to dismiss";
  }, 5000);
}

function renderGameMenu(menu) {
  clearTimeout(menuTimeout);
  const fragment = document.createDocumentFragment();
  const label = document.createElement("div");
  label.className = "module-menu-heading";
  label.textContent = menu.noun || "Actions";
  gameContextMenu.setAttribute("aria-label", `${menu.noun || "Game"} actions`);
  fragment.appendChild(label);
  let actionable = 0;
  menu.items.forEach((item, index) => {
    if (item.disabled) {
      const heading = document.createElement("div");
      heading.className = "module-menu-heading";
      heading.textContent = item.text;
      fragment.appendChild(heading);
      return;
    }
    if (!item.command || /^(?:__|action:|menu:)/.test(item.command)) return;
    const button = document.createElement("button");
    button.type = "button";
    button.setAttribute("role", "menuitem");
    button.textContent = item.text;
    button.addEventListener("click", () => {
      try {
        const result = interaction.pick({ requestId: menu.requestId, index });
        commandStatus.textContent = `Sent (unconfirmed): ${result.label}`;
      } catch (error) {
        commandStatus.textContent = error?.message || "Action was not sent";
      } finally {
        closeGameMenu(true);
      }
    });
    fragment.appendChild(button);
    actionable += 1;
  });
  if (!actionable) {
    const note = document.createElement("div");
    note.className = "empty-state";
    note.textContent = "No actions available";
    fragment.appendChild(note);
  }
  gameContextMenu.replaceChildren(fragment);
  gameContextMenu.hidden = false;
  requestAnimationFrame(() => {
    positionGameMenu();
    gameContextMenu.querySelector("button:not(:disabled)")?.focus();
  });
}

function closeGameMenu(restoreFocus = false) {
  const anchor = menuAnchor;
  clearTimeout(menuTimeout);
  gameContextMenu.hidden = true;
  gameContextMenu.replaceChildren();
  menuAnchor = null;
  interaction?.close();
  if (restoreFocus && anchor?.isConnected) anchor.focus();
}

function paneScrollState(container) {
  let state = textPaneState.get(container);
  if (state) return state;
  state = { follow: true, paused: false, lastSeq: 0 };
  container.addEventListener("scroll", () => {
    state.follow =
      container.scrollTop + container.clientHeight >= container.scrollHeight - 40;
  }, { passive: true });
  textPaneState.set(container, state);
  return state;
}

function renderTextStream(container, entries, emptyMessage) {
  const state = paneScrollState(container);
  if (state.paused) return;
  const rows = Array.isArray(entries) ? entries : [];
  const newest = rows.at(-1)?.seq || 0;
  const reset = newest < state.lastSeq || (rows.length > 0 && state.lastSeq === 0);
  const gapMarker = () => {
    const marker = document.createElement("div");
    marker.className = "text-line output-gap";
    marker.textContent = "— missed output while disconnected —";
    return marker;
  };

  if (rows.length === 0) {
    state.lastSeq = 0;
    if (state.gapPending) {
      container.replaceChildren(gapMarker());
      state.gapPending = false;
    } else {
      empty(container, emptyMessage);
    }
    return;
  }

  if (reset || container.querySelector(".empty-state")) {
    container.replaceChildren();
    const fragment = document.createDocumentFragment();
    if (state.gapPending) fragment.appendChild(gapMarker());
    for (const entry of rows.slice(-MAX_MOUNTED_LINES)) {
      fragment.appendChild(styledLine(entry.line));
    }
    container.appendChild(fragment);
  } else {
    const fragment = document.createDocumentFragment();
    if (state.gapPending) fragment.appendChild(gapMarker());
    for (const entry of rows) {
      if (entry.seq > state.lastSeq) fragment.appendChild(styledLine(entry.line));
    }
    container.appendChild(fragment);
  }
  state.gapPending = false;

  while (container.childElementCount > MAX_MOUNTED_LINES) {
    container.firstElementChild.remove();
  }
  state.lastSeq = newest;
  if (state.follow) container.scrollTop = container.scrollHeight;
}

function renderStyledLines(container, lines, emptyMessage = "No data received") {
  if (!Array.isArray(lines) || lines.length === 0) {
    empty(container, emptyMessage);
    return;
  }
  const fragment = document.createDocumentFragment();
  for (const line of lines) fragment.appendChild(styledLine(line));
  container.replaceChildren(fragment);
}

function stream(view, id) {
  return view.streams?.[id] || [];
}

function registerModules() {
  workspace.register({
    id: "active-spells",
    slices: ["effects"],
    render(view, { body }) {
      renderEffects(body, effectCategory(view.effects, "ActiveSpells"), "No active spells");
    },
  });
  workspace.register({
    id: "known-spells",
    slices: ["spellbook"],
    render(view, { body }) {
      renderStyledLines(body, view.spellbook, "No known-spell data received");
    },
  });
  workspace.register({
    id: "injuries",
    slices: ["injuries", "doll"],
    render(view, { body }) {
      renderInjuries(body, view.injuries);
    },
  });
  workspace.register({
    id: "cooldowns",
    slices: ["effects"],
    render(view, { body }) {
      renderEffects(body, effectCategory(view.effects, "Cooldowns"), "No cooldowns");
    },
  });
  workspace.register({
    id: "story",
    slices: ["streams", "stream:main"],
    render(view, { body }) {
      renderTextStream(body, stream(view, "main"), "No game text received");
    },
  });
  workspace.register({
    id: "thoughts",
    slices: ["streams", "stream:thoughts"],
    render(view, { body }) {
      renderTextStream(body, stream(view, "thoughts"), "No thoughts received");
    },
  });
  workspace.register({
    id: "familiar",
    slices: ["streams", "stream:familiar"],
    render(view, { body }) {
      renderTextStream(body, stream(view, "familiar"), "No familiar messages received");
    },
  });
  workspace.register({
    id: "room",
    slices: ["room", "entities"],
    render(view) {
      const room = view.room || {};
      const roomTitle = document.getElementById("room-title");
      roomTitle.textContent = room.name
        ? `${room.name}${room.id ? ` - ${room.id}` : ""}`
        : "No room data";
      renderStyledLines(
        document.getElementById("room-description"),
        room.description,
        "No room description received",
      );
      const occupants = document.getElementById("room-occupants");
      renderRoomOccupants(occupants, view.entities);
      renderRoomExits(document.getElementById("room-exits"), room.exits || []);
    },
  });
  workspace.register({
    id: "compass",
    slices: ["room"],
    render(view, { body }) {
      const exits = new Set((view.room?.exits || []).map(normalizeExit));
      for (const button of body.querySelectorAll("button[data-direction]")) {
        const direction = normalizeExit(button.dataset.direction);
        const available = exits.has(direction);
        button.disabled = !available;
        button.classList.toggle("available", available);
      }
    },
  });
  workspace.register({
    id: "hands",
    slices: ["hands", "preparedSpell"],
    render(view) {
      document.getElementById("hands-left").textContent = view.hands?.left || "Empty";
      document.getElementById("hands-right").textContent = view.hands?.right || "Empty";
      document.getElementById("hands-prepared").textContent = view.preparedSpell || "None";
    },
  });
  workspace.register({
    id: "vitals",
    slices: ["vitals", "minivitals", "charInfo"],
    render(view) {
      for (const id of ["health", "mana", "stamina", "spirit"]) {
        renderVital(id, view.vitals?.[id] || 0, view.minivitals || []);
      }
      renderCharacterGauges(view.charInfo?.gauges);
    },
  });
  workspace.register({
    id: "conditions",
    slices: ["indicators", "timers"],
    render(view, { body }) {
      renderConditions(body, view);
    },
  });
  workspace.register({
    id: "map",
    slices: ["mapScene", "mapState"],
    render(view) {
      mapController.render({ scene: view.mapScene, state: view.mapState });
    },
  });
  workspace.register({
    id: "combat",
    slices: ["targets", "charInfo", "indicators", "timers"],
    render(view, { body }) {
      renderCombat(
        body,
        view.targets,
        view.charInfo?.gauges?.stance,
        view.indicators,
        view.timers,
      );
    },
  });
  workspace.register({
    id: "tasks",
    slices: ["objectives", "charInfo"],
    render(view, { body }) {
      renderTasks(body, view.objectives, view.charInfo);
    },
  });
  workspace.register({
    id: "inventory",
    slices: ["inventory"],
    render(view, { body }) {
      renderStyledLines(body, view.inventory, "No inventory data received");
    },
  });
}

function effectCategory(categories, name) {
  return (Array.isArray(categories) ? categories : [])
    .find((entry) => entry?.category === name)?.effects || [];
}

function displayDuration(effect) {
  const expiresAt = effect?.expiresAt ?? effect?.expires_at;
  if (typeof expiresAt !== "number") return effect?.time || "";
  const seconds = Math.max(0, Math.floor(expiresAt - serverNow()));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainder = seconds % 60;
  return [hours, minutes, remainder].map((value) => String(value).padStart(2, "0")).join(":");
}

function renderEffects(container, effects, emptyMessage) {
  if (!Array.isArray(effects) || effects.length === 0) {
    empty(container, emptyMessage);
    return;
  }
  const fragment = document.createDocumentFragment();
  for (const effect of effects) {
    const row = document.createElement("div");
    row.className = "effect-entry";
    const label = document.createElement("span");
    label.textContent = effect.id && !String(effect.text || "").startsWith(`${effect.id} `)
      ? `${effect.id} · ${effect.text || "Unknown effect"}`
      : effect.text || effect.id || "Unknown effect";
    if (effect.textColor || effect.text_color) {
      label.style.color = effect.textColor || effect.text_color;
    }
    const time = document.createElement("time");
    time.textContent = displayDuration(effect);
    const track = document.createElement("span");
    track.className = "effect-track";
    const fill = document.createElement("span");
    fill.className = "effect-fill";
    fill.style.setProperty("--effect-percent", `${Math.max(0, Math.min(100, Number(effect.value) || 0))}%`);
    fill.style.setProperty("--effect-color", effect.barColor || effect.bar_color || "var(--amber)");
    track.appendChild(fill);
    row.append(label, time, track);
    fragment.appendChild(row);
  }
  container.replaceChildren(fragment);
}

function displayKey(key) {
  return String(key || "unknown")
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .replace(/[_-]+/g, " ")
    .replace(/^./, (letter) => letter.toUpperCase());
}

function renderInjuries(container, injuries) {
  const entries = Object.entries(injuries || {}).filter(([, level]) => Number(level) > 0);
  if (!entries.length) {
    empty(container, "No injuries reported");
    return;
  }
  const list = document.createElement("div");
  list.className = "injury-list";
  for (const [part, rawLevel] of entries.sort(([a], [b]) => a.localeCompare(b))) {
    const level = Number(rawLevel);
    const scar = level > 3;
    const row = document.createElement("div");
    row.className = `injury-entry ${scar ? "injury-scar" : "injury-wound"}`;
    row.textContent = `${displayKey(part)}: ${scar ? "scar" : "wound"} rank ${scar ? level - 3 : level}`;
    list.appendChild(row);
  }
  container.replaceChildren(list);
}

function renderCharacterGauges(gauges = {}) {
  for (const [id, label] of [
    ["mind", "Mind"],
    ["encumbrance", "Encumbrance"],
    ["stance", "Stance"],
  ]) {
    const output = document.getElementById(`gauge-${id}`);
    const gauge = gauges?.[id];
    if (!output) continue;
    output.textContent = gauge
      ? `${label}: ${gauge.text || gauge.value}${Number.isFinite(gauge.value) ? ` (${gauge.value}%)` : ""}`
      : `${label}: unknown`;
    const value = Number(gauge?.value);
    const danger = id === "stance" ? value < 40 : value >= 75;
    const warn = id === "stance" ? value < 80 : value >= 40;
    output.dataset.level = !Number.isFinite(value)
      ? "unknown"
      : danger ? "danger" : warn ? "warn" : "good";
  }
}

function renderCombat(container, targets, stance, indicators = {}, timers = {}) {
  const fragment = document.createDocumentFragment();
  const actions = document.createElement("div");
  actions.className = "stance-actions";
  const currentStance = String(stance?.text || "").toLowerCase();
  for (const name of ["defensive", "guarded", "neutral", "advance", "offensive"]) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = displayKey(name);
    button.setAttribute("aria-pressed", String(currentStance === name));
    button.addEventListener("click", () => submitCommand(`stance ${name}`, `Stance: ${name}`));
    actions.appendChild(button);
  }
  fragment.appendChild(actions);
  const summary = document.createElement("div");
  summary.className = "combat-summary";
  for (const text of [
    `Stunned: ${indicators?.stunned ? "yes" : "no"}`,
    `Dead: ${indicators?.dead ? "yes" : "no"}`,
    `Roundtime: ${roundtimeRemaining(timers)}s`,
  ]) {
    const line = document.createElement("div");
    line.className = "target-status";
    line.textContent = text;
    summary.appendChild(line);
  }
  fragment.appendChild(summary);
  const rows = Array.isArray(targets) ? targets : [];
  if (!rows.length) {
    const note = document.createElement("p");
    note.className = "empty-state";
    note.textContent = "No current targets";
    fragment.appendChild(note);
  } else {
    const list = document.createElement("div");
    list.className = "target-list";
    for (const target of rows) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "target-entry";
      button.dataset.current = String(Boolean(target.current));
      button.textContent = target.name || target.noun || "Unknown target";
      if (target.status) {
        const status = document.createElement("span");
        status.className = "target-status";
        status.textContent = ` · ${target.status}`;
        button.appendChild(status);
      }
      button.addEventListener("click", () => activateLink({
        exist_id: target.id,
        noun: target.noun || target.name || "",
        text: target.name || target.noun || "",
      }, button));
      list.appendChild(button);
    }
    fragment.appendChild(list);
  }
  container.replaceChildren(fragment);
}

function renderTasks(container, objectives, charInfo = {}) {
  const fragment = document.createDocumentFragment();
  const rows = Array.isArray(objectives?.objectives) ? objectives.objectives : [];
  if (rows.length) {
    const list = document.createElement("div");
    list.className = "objective-list";
    for (const objective of rows) {
      const entry = document.createElement("article");
      entry.className = "objective-entry";
      const title = document.createElement("strong");
      title.textContent = objective.name || objective.kind || "Objective";
      const meta = document.createElement("div");
      meta.className = "objective-meta";
      meta.textContent = [objective.state, objective.location, objective.cadence].filter(Boolean).join(" · ");
      const description = document.createElement("div");
      description.textContent = objective.description || "";
      entry.append(title, meta, description);
      if (Array.isArray(objective.actions) && objective.actions.length) {
        const actions = document.createElement("div");
        actions.className = "objective-actions";
        for (const action of objective.actions) {
          if (!action?.command) continue;
          const button = document.createElement("button");
          button.type = "button";
          button.textContent = displayKey(action.actionType || "Run");
          button.addEventListener("click", () => submitCommand(action.command));
          actions.appendChild(button);
        }
        entry.appendChild(actions);
      }
      list.appendChild(entry);
    }
    fragment.appendChild(list);
  }
  for (const [section, label] of [["bounty", "Bounty"], ["society", "Society"]]) {
    for (const line of Array.isArray(charInfo?.[section]) ? charInfo[section] : []) {
      const row = document.createElement("div");
      row.className = "character-line";
      row.textContent = `${label}: ${line}`;
      fragment.appendChild(row);
    }
  }
  if (!fragment.childNodes.length) {
    empty(container, "No active tasks reported");
  } else {
    container.replaceChildren(fragment);
  }
}

function createMapController(canvas, emptyState, options = {}) {
  if (!canvas) throw new Error("Map canvas is missing");
  const context = canvas.getContext("2d");
  if (!context) throw new Error("Canvas rendering is unavailable");
  const classicStage = options.classicStage;
  const classicImage = options.classicImage;
  const classicMarker = options.classicMarker;
  const selector = options.selector;
  const classicButton = options.classicButton;
  const localButton = options.localButton;
  if (!classicStage || !classicImage || !classicMarker || !selector || !classicButton || !localButton) {
    throw new Error("Map mode controls are missing");
  }
  const viewport = new DesktopMapViewport();
  const events = new AbortController();
  let frame = null;
  let resizeObserver = null;
  let mode = "classic";
  let liveFrame = { scene: null, state: null };
  let localBrowse = null;
  let localLocations = null;
  let classicCatalog = null;
  let classicCatalogRequest = null;
  let mapRequestId = 0;
  let pendingLocationsRequest = 0;
  let pendingBrowseRequest = 0;
  let classicFollowsCurrent = true;
  let classicAutoCenter = true;
  let classicName = null;
  let classicRect = null;
  let classicLoaded = false;
  let classicNeedsFit = false;
  let classicCamera = { x: 0, y: 0, scale: 1 };
  let classicDrag = null;
  let localPointer = null;

  const currentClassic = () => liveFrame.state?.classic || null;
  const classicUrl = (name) => (
    `/api/v1/maps/classic/${encodeURIComponent(name)}?token=${encodeURIComponent(options.token?.() || "")}`
  );
  const setEmpty = (message = null) => {
    emptyState.textContent = message || "";
    emptyState.hidden = !message;
  };
  const setModeControls = () => {
    classicButton.setAttribute("aria-pressed", String(mode === "classic"));
    localButton.setAttribute("aria-pressed", String(mode === "local"));
    canvas.hidden = mode !== "local";
    classicStage.hidden = mode !== "classic";
  };
  const selectorOption = (value, label) => {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    return option;
  };
  const renderSelector = () => {
    const selected = selector.value;
    selector.replaceChildren(selectorOption("", "Current map"));
    if (mode === "classic") {
      for (const entry of classicCatalog || []) {
        if (!entry?.name) continue;
        selector.appendChild(selectorOption(entry.name, entry.label || entry.name));
      }
      if (!classicFollowsCurrent && classicName) selector.value = classicName;
    } else {
      for (const location of localLocations || []) {
        selector.appendChild(selectorOption(location, location));
      }
      if (localBrowse?.location) selector.value = localBrowse.location;
    }
    if (!selector.value && selected && [...selector.options].some((option) => option.value === selected)) {
      selector.value = selected;
    }
  };
  const loadClassicCatalog = async () => {
    if (classicCatalog?.length) return classicCatalog;
    if (classicCatalogRequest) return classicCatalogRequest;
    classicCatalogRequest = (async () => {
      const response = await fetch(
        `/api/v1/maps/classic?token=${encodeURIComponent(options.token?.() || "")}`,
        { cache: "no-store", signal: events.signal },
      );
      if (!response.ok) throw new Error(`classic map catalog returned ${response.status}`);
      const entries = await response.json();
      classicCatalog = Array.isArray(entries) ? entries : [];
      if (mode === "classic") renderSelector();
      return classicCatalog;
    })();
    try {
      return await classicCatalogRequest;
    } catch (error) {
      if (error?.name !== "AbortError") classicCatalog = null;
      return null;
    } finally {
      classicCatalogRequest = null;
    }
  };
  const requestLocalLocations = () => {
    if (localLocations || pendingLocationsRequest) return;
    pendingLocationsRequest = ++mapRequestId;
    try {
      options.requestLocations?.(pendingLocationsRequest);
    } catch {
      pendingLocationsRequest = 0;
    }
  };

  const positionClassic = () => {
    if (!classicLoaded) return;
    const rect = classicStage.getBoundingClientRect();
    const { x, y, scale } = classicCamera;
    const left = rect.width / 2 - x * scale;
    const top = rect.height / 2 - y * scale;
    classicImage.style.transform = `translate(${left}px, ${top}px) scale(${scale})`;
    const current = currentClassic();
    const showMarker = Boolean(current && current.image === classicName && classicRect);
    classicMarker.hidden = !showMarker;
    if (showMarker) {
      const centerX = (classicRect[0] + classicRect[2]) / 2;
      const centerY = (classicRect[1] + classicRect[3]) / 2;
      classicMarker.style.left = `${left + centerX * scale}px`;
      classicMarker.style.top = `${top + centerY * scale}px`;
    }
  };
  const centerClassic = () => {
    if (!classicLoaded) return false;
    const current = currentClassic();
    if (classicFollowsCurrent && current?.image === classicName && classicRect) {
      classicCamera.x = (classicRect[0] + classicRect[2]) / 2;
      classicCamera.y = (classicRect[1] + classicRect[3]) / 2;
    } else {
      classicCamera.x = classicImage.naturalWidth / 2;
      classicCamera.y = classicImage.naturalHeight / 2;
    }
    positionClassic();
    return true;
  };
  const fitClassic = () => {
    if (!classicLoaded) return;
    const rect = classicStage.getBoundingClientRect();
    const fit = Math.min(
      rect.width / Math.max(1, classicImage.naturalWidth),
      rect.height / Math.max(1, classicImage.naturalHeight),
    );
    classicCamera.scale = Math.max(0.05, Math.min(4, fit));
    centerClassic();
  };
  const showClassic = (name, roomRect = null) => {
    if (!name) {
      classicName = null;
      classicRect = null;
      classicLoaded = false;
      classicNeedsFit = false;
      classicImage.removeAttribute("src");
      classicMarker.hidden = true;
      setEmpty("No classic map is available for this room");
      return;
    }
    classicRect = Array.isArray(roomRect) ? roomRect : null;
    if (classicName === name && classicLoaded) {
      if (classicNeedsFit) {
        classicNeedsFit = false;
        fitClassic();
      } else if (classicFollowsCurrent && classicAutoCenter) {
        centerClassic();
      } else {
        positionClassic();
      }
      setEmpty();
      return;
    }
    classicName = name;
    classicLoaded = false;
    classicNeedsFit = true;
    classicMarker.hidden = true;
    setEmpty("Loading classic map…");
    classicImage.alt = `Classic map: ${name}`;
    classicImage.src = classicUrl(name);
  };
  const showCurrentClassic = () => {
    classicFollowsCurrent = true;
    selector.value = "";
    const current = currentClassic();
    if (current && !classicCatalog?.length) loadClassicCatalog();
    showClassic(current?.image || null, current?.roomRect || null);
  };
  const localFrame = () => {
    if (!localBrowse) return liveFrame;
    const rooms = localBrowse.scene?.rooms || [];
    if (!rooms.length) return { scene: localBrowse.scene, state: { available: false } };
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (const room of rooms) {
      minX = Math.min(minX, room.x); maxX = Math.max(maxX, room.x);
      minY = Math.min(minY, room.y); maxY = Math.max(maxY, room.y);
    }
    return {
      scene: localBrowse.scene,
      state: {
        available: true,
        location: localBrowse.location,
        room: null,
        cell: [(minX + maxX) / 2, (minY + maxY) / 2],
        inGhost: false,
        ghosts: [],
        ghostEdges: [],
      },
    };
  };
  const renderLocalFrame = () => {
    viewport.setFrame(localFrame());
    scheduleDraw();
  };
  const selectMode = (next) => {
    mode = next === "local" ? "local" : "classic";
    setModeControls();
    renderSelector();
    if (mode === "classic") {
      loadClassicCatalog();
      if (classicFollowsCurrent) showCurrentClassic();
      else if (classicLoaded && classicNeedsFit) {
        classicNeedsFit = false;
        fitClassic();
      } else positionClassic();
    } else {
      requestLocalLocations();
      renderLocalFrame();
    }
  };

  const draw = () => {
    frame = null;
    if (mode !== "local") return;
    const rect = canvas.getBoundingClientRect();
    const ratio = Math.max(1, window.devicePixelRatio || 1);
    const width = Math.max(1, Math.round(rect.width * ratio));
    const height = Math.max(1, Math.round(rect.height * ratio));
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, rect.width, rect.height);

    const snapshot = viewport.snapshot();
    const { scene, state, camera } = snapshot;
    const available = Boolean(state.available && state.cell && scene);
    setEmpty(available ? null : "No local map data received");
    if (!available) return;

    const point = (x, y) => ({
      x: (x - camera.x) * camera.pixelsPerCell + rect.width / 2,
      y: (y - camera.y) * camera.pixelsPerCell + rect.height / 2,
    });
    const onScreen = ({ x, y }) => (
      x > -camera.pixelsPerCell * 2 &&
      x < rect.width + camera.pixelsPerCell * 2 &&
      y > -camera.pixelsPerCell * 2 &&
      y < rect.height + camera.pixelsPerCell * 2
    );
    const showLabels = camera.pixelsPerCell >= 14;
    const showIds = camera.pixelsPerCell >= 20;
    context.lineCap = "round";
    context.lineJoin = "round";
    for (const edge of scene.edges || []) {
      const a = point(edge.x1, edge.y1);
      const b = point(edge.x2, edge.y2);
      if (!onScreen(a) && !onScreen(b)) continue;
      context.strokeStyle = "#7c8790";
      context.fillStyle = "#7c8790";
      context.lineWidth = 1;
      if (edge.kind === 0 || edge.kind === 1) {
        context.beginPath();
        context.setLineDash(edge.kind === 0 ? [] : [4, 4]);
        context.moveTo(a.x, a.y);
        context.lineTo(b.x, b.y);
        context.stroke();
        if (edge.kind === 1 && showLabels && edge.label) {
          context.setLineDash([]);
          context.font = `${10 * options.fontScale()}px system-ui, sans-serif`;
          context.textAlign = "center";
          context.fillText(edge.label, (a.x + b.x) / 2, (a.y + b.y) / 2);
        }
      } else if (edge.kind === 2) {
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        const length = Math.hypot(dx, dy) || 1;
        const unitX = dx / length;
        const unitY = dy / length;
        const stubLength = camera.pixelsPerCell * 0.9;
        context.setLineDash([4, 3]);
        for (const [origin, direction, partner] of [
          [a, 1, edge.bRoom],
          [b, -1, edge.aRoom],
        ]) {
          context.beginPath();
          context.moveTo(origin.x, origin.y);
          context.lineTo(
            origin.x + unitX * direction * stubLength,
            origin.y + unitY * direction * stubLength,
          );
          context.stroke();
          if (showIds && partner !== null) {
            context.setLineDash([]);
            context.font = `${9 * options.fontScale()}px system-ui, sans-serif`;
            context.textAlign = "center";
            context.fillText(
              String(partner),
              origin.x + unitX * direction * (stubLength + 8),
              origin.y + unitY * direction * (stubLength + 8),
            );
            context.setLineDash([4, 3]);
          }
        }
      } else if (edge.kind === 3) {
        context.setLineDash([]);
        context.fillStyle = "#9b7fd3";
        for (const endpoint of [a, b]) {
          context.beginPath();
          context.arc(endpoint.x, endpoint.y, 3, 0, Math.PI * 2);
          context.fill();
        }
      }
    }
    for (const edge of state.ghostEdges || []) {
      const a = point(edge.x1, edge.y1);
      const b = point(edge.x2, edge.y2);
      if (!onScreen(a) && !onScreen(b)) continue;
      context.beginPath();
      context.setLineDash([3, 4]);
      context.strokeStyle = "#8b7a58";
      context.moveTo(a.x, a.y);
      context.lineTo(b.x, b.y);
      context.stroke();
    }
    context.setLineDash([]);
    const nodeSize = Math.max(6, Math.min(16, camera.pixelsPerCell * 0.62));
    for (const room of scene.rooms || []) {
      const p = point(room.x, room.y);
      if (!onScreen(p)) continue;
      const current = !state.inGhost && room.i === state.room;
      context.fillStyle = current ? "#d7ad63" : "#34414a";
      context.strokeStyle = current ? "#f0c674" : "#87929b";
      context.lineWidth = current ? 2 : 1;
      context.fillRect(p.x - nodeSize / 2, p.y - nodeSize / 2, nodeSize, nodeSize);
      context.strokeRect(p.x - nodeSize / 2, p.y - nodeSize / 2, nodeSize, nodeSize);
      if (room.entrance) {
        context.fillStyle = "#f0c674";
        context.fillRect(p.x - 1, p.y - nodeSize / 2 - 4, 2, 3);
      }
    }
    for (const ghost of state.ghosts || []) {
      const p = point(ghost.x, ghost.y);
      if (!onScreen(p)) continue;
      context.beginPath();
      context.arc(p.x, p.y, nodeSize / 2, 0, Math.PI * 2);
      context.fillStyle = ghost.current ? "#d7ad63" : "#302a20";
      context.strokeStyle = "#b99b61";
      context.fill();
      context.stroke();
    }
    context.fillStyle = "#aeb6bc";
    context.font = `${10 * options.fontScale()}px system-ui, sans-serif`;
    context.textAlign = "center";
    for (const label of scene.labels || []) {
      const p = point(label.x, label.y);
      if (!onScreen(p)) continue;
      context.fillText(label.text || "", p.x, p.y - 9);
    }
  };

  const scheduleDraw = () => {
    if (frame !== null) return;
    frame = requestAnimationFrame(draw);
  };
  const eventPosition = (event) => {
    const rect = canvas.getBoundingClientRect();
    return { x: event.clientX - rect.left, y: event.clientY - rect.top, width: rect.width, height: rect.height };
  };

  canvas.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || event.isPrimary === false) return;
    localPointer = {
      id: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      moved: false,
    };
    if (viewport.beginDrag({ pointerId: event.pointerId, x: event.clientX, y: event.clientY })) {
      try {
        canvas.setPointerCapture?.(event.pointerId);
      } catch {
        viewport.endDrag(event.pointerId);
        localPointer = null;
        return;
      }
      scheduleDraw();
    }
  }, { signal: events.signal });
  canvas.addEventListener("pointermove", (event) => {
    if (localPointer?.id === event.pointerId) {
      const distance = Math.hypot(
        event.clientX - localPointer.startX,
        event.clientY - localPointer.startY,
      );
      if (distance > 4) localPointer.moved = true;
    }
    if (viewport.dragTo({ pointerId: event.pointerId, x: event.clientX, y: event.clientY })) {
      scheduleDraw();
    }
  }, { signal: events.signal });
  const endDrag = (event) => {
    const wasTap = event.type === "pointerup" &&
      localPointer?.id === event.pointerId &&
      !localPointer.moved;
    localPointer = null;
    if (viewport.endDrag(event.pointerId)) scheduleDraw();
    if (!wasTap || mode !== "local") return;
    const position = eventPosition(event);
    const roomId = viewport.roomAtViewportPoint({
      x: position.x,
      y: position.y,
      width: position.width,
      height: position.height,
    });
    if (roomId === null) return;
    if (event.ctrlKey || event.metaKey) options.reportRoom?.(roomId);
    else options.travelToRoom?.(roomId);
  };
  canvas.addEventListener("pointerup", endDrag, { signal: events.signal });
  canvas.addEventListener("pointercancel", endDrag, { signal: events.signal });
  canvas.addEventListener("wheel", (event) => {
    event.preventDefault();
    if (viewport.zoomWheel({ deltaY: event.deltaY, ...eventPosition(event) })) scheduleDraw();
  }, { passive: false, signal: events.signal });
  canvas.addEventListener("keydown", (event) => {
    const deltaY = event.key === "+" || event.key === "=" ? -1 : event.key === "-" ? 1 : 0;
    if (deltaY) {
      event.preventDefault();
      if (viewport.zoomWheel({ deltaY })) scheduleDraw();
    } else if (event.key === "Home") {
      event.preventDefault();
      if (viewport.center()) scheduleDraw();
    }
  }, { signal: events.signal });

  classicImage.addEventListener("load", () => {
    classicLoaded = true;
    if (mode !== "classic") {
      classicNeedsFit = true;
      return;
    }
    setEmpty();
    classicNeedsFit = false;
    fitClassic();
  }, { signal: events.signal });
  classicImage.addEventListener("error", () => {
    classicLoaded = false;
    classicNeedsFit = false;
    classicMarker.hidden = true;
    if (mode === "classic") setEmpty("Classic map image could not be loaded");
  }, { signal: events.signal });
  classicStage.addEventListener("pointerdown", (event) => {
    classicDrag = { id: event.pointerId, x: event.clientX, y: event.clientY };
    try {
      classicStage.setPointerCapture?.(event.pointerId);
    } catch {
      classicDrag = null;
    }
  }, { signal: events.signal });
  classicStage.addEventListener("pointermove", (event) => {
    if (!classicDrag || classicDrag.id !== event.pointerId || !classicLoaded) return;
    const dx = event.clientX - classicDrag.x;
    const dy = event.clientY - classicDrag.y;
    classicDrag = { id: event.pointerId, x: event.clientX, y: event.clientY };
    classicCamera.x -= dx / classicCamera.scale;
    classicCamera.y -= dy / classicCamera.scale;
    classicAutoCenter = false;
    positionClassic();
  }, { signal: events.signal });
  const endClassicDrag = (event) => {
    if (classicDrag?.id === event.pointerId) classicDrag = null;
  };
  classicStage.addEventListener("pointerup", endClassicDrag, { signal: events.signal });
  classicStage.addEventListener("pointercancel", endClassicDrag, { signal: events.signal });
  classicStage.addEventListener("wheel", (event) => {
    if (!classicLoaded || !Number.isFinite(event.deltaY) || event.deltaY === 0) return;
    event.preventDefault();
    const rect = classicStage.getBoundingClientRect();
    const oldScale = classicCamera.scale;
    const newScale = Math.max(0.05, Math.min(4, oldScale * (event.deltaY < 0 ? 1.15 : 1 / 1.15)));
    const offsetX = event.clientX - rect.left - rect.width / 2;
    const offsetY = event.clientY - rect.top - rect.height / 2;
    const anchorX = classicCamera.x + offsetX / oldScale;
    const anchorY = classicCamera.y + offsetY / oldScale;
    classicCamera.scale = newScale;
    classicCamera.x = anchorX - offsetX / newScale;
    classicCamera.y = anchorY - offsetY / newScale;
    classicAutoCenter = false;
    positionClassic();
  }, { passive: false, signal: events.signal });
  classicStage.addEventListener("keydown", (event) => {
    if (event.key === "Home") {
      event.preventDefault();
      classicAutoCenter = true;
      centerClassic();
    }
  }, { signal: events.signal });

  selector.addEventListener("focus", () => {
    if (mode === "classic") loadClassicCatalog();
    else requestLocalLocations();
  }, { signal: events.signal });
  selector.addEventListener("change", () => {
    if (mode === "classic") {
      if (!selector.value) {
        classicAutoCenter = true;
        showCurrentClassic();
      } else {
        classicFollowsCurrent = false;
        classicAutoCenter = false;
        const current = currentClassic();
        showClassic(selector.value, current?.image === selector.value ? current.roomRect : null);
      }
      return;
    }
    if (!selector.value) {
      pendingBrowseRequest = 0;
      localBrowse = null;
      renderLocalFrame();
      return;
    }
    pendingBrowseRequest = ++mapRequestId;
    try {
      options.requestLocalMap?.(pendingBrowseRequest, selector.value);
    } catch {
      pendingBrowseRequest = 0;
    }
  }, { signal: events.signal });
  classicButton.addEventListener("click", () => selectMode("classic"), { signal: events.signal });
  localButton.addEventListener("click", () => selectMode("local"), { signal: events.signal });
  document.getElementById("map-center")?.addEventListener("click", () => {
    if (mode === "classic") {
      classicAutoCenter = true;
      showCurrentClassic();
    } else {
      pendingBrowseRequest = 0;
      localBrowse = null;
      selector.value = "";
      viewport.setFrame(liveFrame);
      if (viewport.center()) scheduleDraw();
      else scheduleDraw();
    }
  }, { signal: events.signal });
  if (typeof ResizeObserver === "function") {
    resizeObserver = new ResizeObserver(() => {
      if (mode === "local") scheduleDraw();
      else positionClassic();
    });
    resizeObserver.observe(canvas.parentElement);
  } else {
    window.addEventListener("resize", () => {
      if (mode === "local") scheduleDraw();
      else positionClassic();
    }, { signal: events.signal });
  }

  setModeControls();
  renderSelector();
  loadClassicCatalog();

  return Object.freeze({
    refreshTypography() {
      scheduleDraw();
    },
    render(next) {
      liveFrame = next || { scene: null, state: null };
      if (mode === "classic") {
        if (classicFollowsCurrent) {
          showCurrentClassic();
        } else {
          const current = currentClassic();
          classicRect = current?.image === classicName ? current.roomRect : null;
          positionClassic();
        }
      } else if (!localBrowse) {
        requestLocalLocations();
        renderLocalFrame();
      }
    },
    center() {
      if (mode === "classic") showCurrentClassic();
      else if (viewport.center()) scheduleDraw();
    },
    receiveLocations({ requestId, locations }) {
      if (requestId !== pendingLocationsRequest) return false;
      pendingLocationsRequest = 0;
      localLocations = Array.isArray(locations) ? [...locations] : [];
      if (mode === "local") renderSelector();
      return true;
    },
    receiveBrowse({ requestId, location, scene, error }) {
      if (requestId !== pendingBrowseRequest) return false;
      pendingBrowseRequest = 0;
      if (error || !scene) {
        setEmpty(error || "No local map is available for that location");
        return false;
      }
      localBrowse = { location, scene };
      if (mode === "local") {
        renderSelector();
        renderLocalFrame();
      }
      return true;
    },
    resetRequests() {
      pendingLocationsRequest = 0;
      pendingBrowseRequest = 0;
      localLocations = null;
      localBrowse = null;
      if (mode === "local") {
        renderSelector();
        renderLocalFrame();
      }
    },
    destroy() {
      events.abort();
      resizeObserver?.disconnect();
      if (frame !== null) cancelAnimationFrame(frame);
      frame = null;
    },
  });
}

function normalizeExit(exit) {
  const key = String(exit || "").trim().toLowerCase();
  return EXIT_ALIASES[key] || key;
}

function renderRoomExits(container, exits) {
  container.replaceChildren();
  if (!exits.length) {
    empty(container, "No obvious exits");
    return;
  }
  for (const rawExit of exits) {
    const command = normalizeExit(rawExit);
    const button = document.createElement("button");
    button.type = "button";
    button.className = "inline-action";
    button.textContent = rawExit;
    button.addEventListener("click", () => submitCommand(command, `Moving ${rawExit}`));
    container.appendChild(button);
  }
}

function renderRoomOccupants(container, entities = {}) {
  const groups = [
    ["creatures", "room-creature"],
    ["players", "room-player"],
    ["objects", "room-object"],
  ];
  const fragment = document.createDocumentFragment();
  let count = 0;
  for (const [key, className] of groups) {
    for (const entity of entities[key] || []) {
      if (count > 0) fragment.appendChild(document.createTextNode(", "));
      const node = document.createElement("span");
      node.className = `room-entity game-link ${className}`;
      node.textContent = entity.label || entity.noun || "unknown";
      node.tabIndex = 0;
      node.setAttribute("role", "link");
      const activate = () => activateLink({
        exist_id: entity.id,
        noun: entity.noun || entity.label || "",
        text: entity.label || entity.noun || "",
      }, node);
      node.addEventListener("click", activate);
      node.addEventListener("keydown", (event) => {
        if (event.key !== "Enter" && event.key !== " ") return;
        event.preventDefault();
        activate();
      });
      fragment.appendChild(node);
      count += 1;
    }
  }
  if (count === 0) {
    empty(container, "None");
  } else {
    container.replaceChildren(fragment);
  }
}

function renderVital(id, percentage, absoluteVitals) {
  const row = document.getElementById(`vital-${id}`);
  const absolute = absoluteVitals.find((entry) => entry.id === id);
  const safePercentage = Math.max(0, Math.min(100, Number(percentage) || 0));
  row.style.setProperty("--vital-percent", `${safePercentage}%`);
  row.dataset.current = String(absolute?.value ?? safePercentage);
  row.dataset.maximum = String(absolute?.max ?? 100);
  row.querySelector("output").textContent = absolute
    ? `${absolute.value} / ${absolute.max}`
    : `${safePercentage}%`;
}

function conditionPill(label, className, active = true) {
  const pill = document.createElement("span");
  pill.className = `condition-pill ${className}${active ? " active" : ""}`;
  pill.textContent = label;
  return pill;
}

function renderConditions(container, view) {
  const flags = view.indicators || {};
  const posture = flags.standing
    ? ["STANDING", "condition-good"]
    : flags.kneeling
      ? ["KNEELING", "condition-warn"]
      : flags.sitting
        ? ["SITTING", "condition-warn"]
        : flags.prone
          ? ["PRONE", "condition-danger"]
          : ["POSTURE: —", "condition-muted"];
  const fragment = document.createDocumentFragment();
  fragment.appendChild(conditionPill(posture[0], posture[1]));
  fragment.appendChild(roundtimePill(view.timers));
  for (const [key, label, className] of [
    ["stunned", "STUNNED", "condition-stunned"],
    ["bleeding", "BLEEDING", "condition-danger"],
    ["hidden", "HIDDEN", "condition-info"],
    ["invisible", "INVISIBLE", "condition-info"],
    ["dead", "DEAD", "condition-danger"],
    ["webbed", "WEBBED", "condition-warn"],
    ["poisoned", "POISONED", "condition-danger"],
    ["diseased", "DISEASED", "condition-danger"],
  ]) {
    fragment.appendChild(conditionPill(label, className, Boolean(flags[key])));
  }
  container.replaceChildren(fragment);
}

function serverNow(timers) {
  return Math.floor(Date.now() / 1000) + clockOffsetSeconds;
}

function calibrateClock(timers) {
  if (typeof timers?.serverTime === "number" && timers.serverTime > 0) {
    clockOffsetSeconds = timers.serverTime - Math.floor(Date.now() / 1000);
  }
}

function roundtimeRemaining(timers) {
  const end = Math.max(timers?.roundtimeEnd || 0, timers?.casttimeEnd || 0);
  return Math.max(0, end - serverNow(timers));
}

function roundtimePill(timers) {
  const remaining = roundtimeRemaining(timers);
  const cast = (timers?.casttimeEnd || 0) >= (timers?.roundtimeEnd || 0);
  return conditionPill(
    remaining ? `${cast ? "CT" : "RT"}: ${remaining}s` : "RT: READY",
    remaining ? "condition-danger" : "condition-good",
  );
}

function isPlayable(view) {
  const transportReady = view.connection?.status === "connected";
  const gameState = view.session?.state;
  const gameReady = !view.session?.session_control || gameState === "connected";
  return transportReady && gameReady;
}

function renderConnection(view) {
  const transport = view.connection || { status: "idle" };
  const game = view.session?.session_control ? view.session.state : null;
  const state = transport.status === "connected" && game && game !== "connected"
    ? game
    : transport.status;
  connectionStatus.dataset.state = state;
  connectionStatus.textContent = state === "connected"
    ? "Connected"
    : state === "reconnecting"
      ? `Reconnecting${transport.attempt ? ` (${transport.attempt})` : ""}`
      : state === "denied"
        ? "Pairing required"
        : state.charAt(0).toUpperCase() + state.slice(1);
  characterTitle.textContent = characterTitleText(view);
  document.title = characterTitle.textContent;
}

function renderCommandAvailability(view) {
  const ready = isPlayable(view);
  commandInput.disabled = !ready;
  commandButton.disabled = !ready;
  sessionExitButton.disabled = !ready;
  if (ready && commandStatus.textContent === "Waiting for connection") {
    commandStatus.textContent = "Connected";
  }
}

sessionExitButton.addEventListener("click", () => {
  if (!latestView || !isPlayable(latestView)) return;
  const confirmed = window.confirm(
    "Exit this Vellum session and log the character out of the game?",
  );
  if (!confirmed) return;
  try {
    session.dispatch({ kind: "exit-and-logout" });
    sessionExitButton.disabled = true;
    commandInput.disabled = true;
    commandButton.disabled = true;
    commandStatus.textContent = "Exit requested; waiting for the game to log out.";
  } catch (error) {
    commandStatus.textContent = error?.message || "Exit request was not sent";
    renderCommandAvailability(latestView);
  }
});

function submitCommand(text, status = null) {
  try {
    const receipt = interaction.submit(text);
    commandStatus.textContent = `Sent (unconfirmed): ${status || text}`;
    return receipt;
  } catch (error) {
    commandStatus.textContent = error?.message || "Command was not sent";
    return null;
  }
}

for (const button of document.querySelectorAll("#compass button[data-direction]")) {
  button.addEventListener("click", () => {
    submitCommand(normalizeExit(button.dataset.direction), `Moving ${button.textContent}`);
  });
}

commandForm.addEventListener("submit", (event) => {
  event.preventDefault();
  const text = commandInput.value.trim();
  if (!text || !submitCommand(text)) return;
  if (commandHistory.at(-1) !== text) commandHistory.push(text);
  commandHistoryIndex = commandHistory.length;
  commandInput.value = "";
});

commandInput.addEventListener("keydown", (event) => {
  if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
  if (!commandHistory.length) return;
  event.preventDefault();
  commandHistoryIndex += event.key === "ArrowUp" ? -1 : 1;
  commandHistoryIndex = Math.max(0, Math.min(commandHistory.length, commandHistoryIndex));
  commandInput.value = commandHistory[commandHistoryIndex] || "";
  commandInput.setSelectionRange(commandInput.value.length, commandInput.value.length);
});

const storyOutput = document.getElementById("story-output");
const storyPause = document.getElementById("story-pause");
const storyBottom = document.getElementById("story-bottom");

function setStoryPaused(paused) {
  const state = paneScrollState(storyOutput);
  state.paused = paused;
  storyPause.setAttribute("aria-pressed", String(paused));
  storyPause.textContent = paused ? "Resume" : "Pause";
  if (!paused && latestView) {
    state.follow = true;
    workspace.render(latestView, ["streams", "stream:main"]);
  }
}

storyPause.addEventListener("click", () => {
  setStoryPaused(!paneScrollState(storyOutput).paused);
});
storyBottom.addEventListener("click", () => {
  const state = paneScrollState(storyOutput);
  state.follow = true;
  setStoryPaused(false);
  storyOutput.scrollTop = storyOutput.scrollHeight;
});

registerModules();

const session = new DesktopSession({
  location: window.location,
  storage: window.localStorage,
});

function adoptVellumPairingToken(event) {
  if (event.key !== VELLUM_TOKEN_STORAGE_KEY || !event.newValue) return;
  try {
    session.replacePairingToken(event.newValue);
  } catch (error) {
    commandStatus.textContent = error?.message || "Pairing token was not accepted";
  }
}
window.addEventListener("storage", adoptVellumPairingToken);
interaction = new DesktopInteractionCoordinator({
  dispatch(intent) {
    return session.dispatch(intent);
  },
  submit(command) {
    const receipt = session.dispatch({ kind: "submit-text", text: command });
    commandStatus.textContent = `Sent (unconfirmed): ${command}`;
    return receipt;
  },
  isOnline() {
    return Boolean(latestView && isPlayable(latestView));
  },
  openUrl(url) {
    window.open(url, "_blank", "noopener,noreferrer");
  },
  reserveUrl() {
    const target = window.open("about:blank", "_blank");
    if (!target) return null;
    target.opener = null;
    return {
      navigate(url) {
        target.location.replace(url);
      },
      close() {
        if (!target.closed) target.close();
      },
    };
  },
});

document.addEventListener("pointerdown", (event) => {
  if (gameContextMenu.hidden) return;
  if (gameContextMenu.contains(event.target) || menuAnchor?.contains?.(event.target)) return;
  closeGameMenu();
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !gameContextMenu.hidden) closeGameMenu(true);
});

session.subscribe((event) => {
  const view = event.state;
  latestView = view;
  renderIdleSurface(view);
  workspace.setCharacter(view.character || view.session?.character);
  if (
    event.type === "snapshot" ||
    (event.type === "state" && event.changed?.includes("timers"))
  ) {
    calibrateClock(view.timers);
  }
  renderConnection(view);
  renderCommandAvailability(view);
  if (!isPlayable(view)) interaction.cancelPendingUrls();
  switch (event.type) {
    case "snapshot":
      closeGameMenu();
      mapController.resetRequests();
      workspace.render(view, null);
      break;
    case "reset":
      closeGameMenu();
      mapController.resetRequests();
      commandHistory.length = 0;
      commandHistoryIndex = 0;
      commandInput.value = "";
      commandStatus.textContent = "Waiting for connection";
      setStoryPaused(false);
      workspace.render(view, null);
      break;
    case "text":
      workspace.render(view, ["streams", `stream:${event.stream}`]);
      break;
    case "state":
      workspace.render(view, event.changed);
      break;
    case "connection":
      workspace.render(view, ["connection", "session"]);
      if (!isPlayable(view)) {
        closeGameMenu();
        mapController.resetRequests();
      }
      break;
    case "menu": {
      try {
        const result = interaction.receiveMenu(event.menu);
        if (result.type === "menu") renderGameMenu(result.menu);
      } catch (error) {
        commandStatus.textContent = error?.message || "Menu could not be loaded";
        closeGameMenu();
      }
      break;
    }
    case "open-url": {
      try {
        const result = interaction.receiveOpenUrl(event.url);
        commandStatus.textContent = result.dropped
          ? "Skill manager window was not opened"
          : result.reserved
            ? "Opened skill manager"
            : "Requested skill manager window";
      } catch (error) {
        commandStatus.textContent = error?.message || "Skill manager was not opened";
      }
      break;
    }
    case "map-locations":
      mapController.receiveLocations(event);
      break;
    case "map-browse":
      mapController.receiveBrowse(event);
      break;
    case "gap": {
      paneScrollState(storyOutput).gapPending = true;
      break;
    }
    case "error":
      commandStatus.textContent = event.error?.message || "Session error";
      break;
    case "dispatch-uncertain":
      commandStatus.textContent = event.message ||
        "The last command or action may not have reached the game and was not replayed.";
      break;
  }
});

session.connect();

// Keep the RT/CT text current between game prompts without rerendering any
// unrelated module.
let timerTicks = 0;
const timerTick = setInterval(() => {
  if (!latestView) return;
  timerTicks += 1;
  workspace.render(latestView, timerTicks % 4 === 0 ? ["timers", "effects"] : ["timers"]);
}, 250);

// Session.js has already persisted the token by this point. Remove it from the
// address bar so it is not copied or retained in browser history.
if (/(?:^#|&)token=/i.test(window.location.hash)) {
  history.replaceState(null, "", window.location.pathname);
}

window.addEventListener("pagehide", (event) => {
  interaction.cancelPendingUrls();
  if (event.persisted) return;
  workspace.flush();
  window.removeEventListener("storage", adoptVellumPairingToken);
  clearInterval(timerTick);
  closeGameMenu();
  gameContextMenu.remove();
  mapController.destroy();
  session.close();
  workspace.destroy();
});
