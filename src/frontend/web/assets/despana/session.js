/**
 * DOM-independent session adapter for Vellum's WebSocket protocol v1.
 *
 * Invariants:
 * - auth is always the first outbound frame; play subscription precedes resume;
 * - a changed server epoch clears every state slice and the text cursor;
 * - only text is de-duplicated by seq (state frames sharing a seq still apply);
 * - dispatch has no outbox and never retries a command after reconnect;
 * - listeners receive state after reduction, in wire order.
 */

const PROTOCOL_VERSION = 1;
export const VELLUM_TOKEN_STORAGE_KEY = "vellum-token";
const DEFAULT_MAX_LINES_PER_STREAM = 2000;
const INITIAL_RECONNECT_MS = 1000;
const MAX_RECONNECT_MS = 10000;

const TRACKED_STATE_DELTAS = new Map([
  ["session", "session"],
  ["room", "room"],
  ["hands", "hands"],
  ["vitals", "vitals"],
  ["minivitals", "minivitals"],
  ["indicators", "indicators"],
  ["rt", "timers"],
  ["prepared_spell", "preparedSpell"],
  ["entities", "entities"],
  ["effects", "effects"],
  ["spells", "spellbook"],
  ["inventory", "inventory"],
  ["injuries", "injuries"],
  ["doll", "doll"],
  ["targets", "targets"],
  ["field", "field"],
  ["objectives", "objectives"],
  ["charinfo", "charInfo"],
  ["map_scene", "mapScene"],
  ["map_state", "mapState"],
]);

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function finiteInteger(value, fallback = 0) {
  return Number.isSafeInteger(value) && value >= 0 ? value : fallback;
}

function nullableString(value) {
  return typeof value === "string" && value.length > 0 ? value : null;
}

function freezeArray(value) {
  return Object.freeze(Array.isArray(value) ? [...value] : []);
}

function freezeRecord(value) {
  return Object.freeze(isRecord(value) ? { ...value } : {});
}

function stringValue(value) {
  return typeof value === "string" ? value : "";
}

function stringArray(value) {
  return Object.freeze((Array.isArray(value) ? value : []).filter(
    (entry) => typeof entry === "string",
  ));
}

function despanaMapLocations(value) {
  return Object.freeze(stringArray(value).filter(
    (entry) => !/^sat(?:id)?-\d+$/i.test(entry),
  ));
}

function nullableInteger(value) {
  return Number.isSafeInteger(value) ? value : null;
}

function finiteTuple(value, length) {
  if (!Array.isArray(value) || value.length !== length) return null;
  if (!value.every((entry) => typeof entry === "number" && Number.isFinite(entry))) {
    return null;
  }
  return Object.freeze([...value]);
}

function normalizeSession(value) {
  if (!isRecord(value)) {
    return Object.freeze({ state: "connected", session_control: false });
  }
  return Object.freeze({ ...value });
}

/**
 * Whether /despana should show its native idle-session handoff.
 *
 * A controlled headless session owns the game-session state machine. Its
 * reconnecting state deliberately leaves Despana visible because retained
 * game text is still useful while the supervisor retries. A sidecar session
 * has no web login lifecycle, so once identified it always remains on
 * Despana even if its WebSocket transport is reconnecting.
 */
export function shouldShowVellumIdle(view) {
  if (view?.connection?.status === "denied") return true;
  const session = isRecord(view?.session) ? view.session : null;
  if (!session) return true;
  if (session.session_control !== true) return false;
  return session.state !== "connected" && session.state !== "reconnecting";
}

/** Build the window title without hiding one known identity field behind another. */
export function characterTitleText(view) {
  const name = nullableString(view?.character) || nullableString(view?.session?.character);
  const profession = nullableString(view?.charInfo?.profession);
  const level = nullableString(view?.charInfo?.level);
  const parts = ["Vellum Despana"];
  if (name) {
    parts.push(name);
    if (profession) parts.push(profession);
    if (level) parts.push(level);
  }
  return parts.join(" - ");
}

function normalizeRoom(value) {
  const room = isRecord(value) ? value : {};
  return Object.freeze({
    name: nullableString(room.name),
    id: nullableString(room.id),
    exits: freezeArray(room.exits),
    description: freezeArray(room.description),
  });
}

function normalizeHands(value) {
  const hands = isRecord(value) ? value : {};
  return Object.freeze({
    left: nullableString(hands.left),
    right: nullableString(hands.right),
  });
}

function percent(value) {
  if (typeof value !== "number" || !Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(100, Math.trunc(value)));
}

function normalizeVitals(value) {
  const vitals = isRecord(value) ? value : {};
  return Object.freeze({
    health: percent(vitals.health),
    mana: percent(vitals.mana),
    stamina: percent(vitals.stamina),
    spirit: percent(vitals.spirit),
  });
}

function normalizeMiniVitals(value) {
  if (!Array.isArray(value)) return Object.freeze([]);
  const vitals = value.flatMap((entry) => {
    if (!isRecord(entry) || typeof entry.id !== "string") return [];
    if (!Number.isSafeInteger(entry.value) || entry.value < 0) return [];
    if (!Number.isSafeInteger(entry.max) || entry.max < 0) return [];
    return [Object.freeze({ id: entry.id, value: entry.value, max: entry.max })];
  });
  return Object.freeze(vitals);
}

function normalizeTimers(value) {
  const timers = isRecord(value) ? value : {};
  return Object.freeze({
    roundtimeEnd:
      typeof timers.roundtime_end === "number" ? timers.roundtime_end : null,
    casttimeEnd:
      typeof timers.casttime_end === "number" ? timers.casttime_end : null,
    serverTime:
      typeof timers.server_time === "number" ? timers.server_time : 0,
  });
}

function normalizeEntities(value) {
  const entities = isRecord(value) ? value : {};
  const normalizeList = (entries) => Object.freeze(
    (Array.isArray(entries) ? entries : []).flatMap((entry) => {
      if (!isRecord(entry) || typeof entry.id !== "string") return [];
      return [Object.freeze({
        id: entry.id,
        label: typeof entry.label === "string" ? entry.label : "",
        noun: typeof entry.noun === "string" ? entry.noun : "",
      })];
    }),
  );
  return Object.freeze({
    creatures: normalizeList(entities.creatures),
    objects: normalizeList(entities.objects),
    players: normalizeList(entities.players),
  });
}

function normalizeEffects(value) {
  const categories = (Array.isArray(value) ? value : []).flatMap((category) => {
    if (!isRecord(category) || typeof category.category !== "string") return [];
    const effects = (Array.isArray(category.effects) ? category.effects : []).flatMap((effect) => {
      if (!isRecord(effect) || typeof effect.id !== "string") return [];
      return [Object.freeze({
        id: effect.id,
        text: stringValue(effect.text),
        value: finiteInteger(effect.value),
        time: stringValue(effect.time),
        expiresAt: nullableInteger(effect.expires_at),
        barColor: nullableString(effect.bar_color),
        textColor: nullableString(effect.text_color),
      })];
    });
    return [Object.freeze({
      category: category.category,
      effects: Object.freeze(effects),
      generation: finiteInteger(category.generation),
    })];
  });
  return Object.freeze(categories);
}

function normalizeInjuries(value) {
  const injuries = {};
  if (isRecord(value)) {
    for (const [part, level] of Object.entries(value)) {
      if (part && Number.isSafeInteger(level) && level >= 1 && level <= 6) {
        injuries[part] = level;
      }
    }
  }
  return Object.freeze(injuries);
}

function normalizeDoll(value) {
  const doll = isRecord(value) ? value : {};
  return Object.freeze({
    variant: nullableString(doll.variant),
    hidden: stringArray(doll.hidden),
  });
}

function normalizeTargets(value) {
  return Object.freeze((Array.isArray(value) ? value : []).flatMap((target) => {
    if (!isRecord(target) || typeof target.id !== "string") return [];
    return [Object.freeze({
      id: target.id,
      name: stringValue(target.name),
      noun: nullableString(target.noun),
      status: nullableString(target.status),
      current: target.current === true,
    })];
  }));
}

function normalizeField(value) {
  return Object.freeze((Array.isArray(value) ? value : []).flatMap((card) => {
    if (!isRecord(card) || typeof card.id !== "string") return [];
    const rect = finiteTuple(card.rect, 4);
    const foot = finiteTuple(card.foot, 2);
    if (!rect || !foot) return [];
    return [Object.freeze({
      id: card.id,
      noun: stringValue(card.noun),
      name: stringValue(card.name),
      rect,
      foot,
      dead: card.dead === true,
      boss: card.boss === true,
      current: card.current === true,
      statuses: stringArray(card.statuses),
      lift: typeof card.lift === "number" && Number.isFinite(card.lift)
        ? card.lift
        : null,
    })];
  }));
}

function normalizeObjectives(value) {
  const content = isRecord(value) ? value : {};
  const objectives = (Array.isArray(content.objectives) ? content.objectives : [])
    .flatMap((objective) => {
      if (!isRecord(objective) || typeof objective.id !== "string") return [];
      const rewards = (Array.isArray(objective.rewards) ? objective.rewards : [])
        .flatMap((reward) => {
          if (!isRecord(reward) || typeof reward.reward_type !== "string") return [];
          return [Object.freeze({
            rewardType: reward.reward_type,
            amount: finiteInteger(reward.amount),
          })];
        });
      const actions = (Array.isArray(objective.actions) ? objective.actions : [])
        .flatMap((action) => {
          if (!isRecord(action) || typeof action.action_type !== "string") return [];
          return [Object.freeze({
            actionType: action.action_type,
            command: stringValue(action.cmd),
          })];
        });
      return [Object.freeze({
        id: objective.id,
        kind: stringValue(objective.kind),
        state: stringValue(objective.state),
        name: stringValue(objective.name),
        description: stringValue(objective.description),
        location: nullableString(objective.location),
        cadence: nullableString(objective.cadence),
        rewards: Object.freeze(rewards),
        actions: Object.freeze(actions),
      })];
    });
  return Object.freeze({
    objectives: Object.freeze(objectives),
    generation: finiteInteger(content.generation),
  });
}

function normalizeGauge(value) {
  if (!isRecord(value) || !Number.isSafeInteger(value.value) || value.value < 0) {
    return null;
  }
  return Object.freeze({ value: value.value, text: stringValue(value.text) });
}

function normalizeCharInfo(value) {
  const info = isRecord(value) ? value : {};
  const gauges = isRecord(info.gauges) ? info.gauges : {};
  const rawFieldExp = isRecord(gauges.field_exp) ? gauges.field_exp : null;
  const fieldExp = rawFieldExp && Number.isSafeInteger(rawFieldExp.value)
    && rawFieldExp.value >= 0 && Number.isSafeInteger(rawFieldExp.max)
    && rawFieldExp.max > 0
    ? Object.freeze({ value: rawFieldExp.value, max: rawFieldExp.max })
    : null;
  return Object.freeze({
    profession: nullableString(info.profession),
    level: nullableString(info.level),
    experience: stringArray(info.experience),
    encumbrance: stringArray(info.encumbrance),
    bounty: stringArray(info.bounty),
    society: stringArray(info.society),
    gauges: Object.freeze({
      mind: normalizeGauge(gauges.mind),
      encumbrance: normalizeGauge(gauges.encumbrance),
      stance: normalizeGauge(gauges.stance),
      fieldExp,
    }),
  });
}

function normalizeMapScene(value) {
  if (!isRecord(value)) return null;
  const rooms = (Array.isArray(value.rooms) ? value.rooms : []).flatMap((room) => {
    if (!isRecord(room) || !Number.isSafeInteger(room.i)) return [];
    if (!Number.isSafeInteger(room.x) || !Number.isSafeInteger(room.y)) return [];
    return [Object.freeze({ i: room.i, x: room.x, y: room.y, entrance: room.e === true })];
  });
  const edges = (Array.isArray(value.edges) ? value.edges : []).flatMap((edge) => {
    if (!isRecord(edge)) return [];
    const coordinates = [edge.x1, edge.y1, edge.x2, edge.y2];
    if (!coordinates.every(Number.isSafeInteger)) return [];
    return [Object.freeze({
      x1: edge.x1,
      y1: edge.y1,
      x2: edge.x2,
      y2: edge.y2,
      kind: finiteInteger(edge.k),
      label: nullableString(edge.l),
      aRoom: nullableInteger(edge.ar),
      bRoom: nullableInteger(edge.br),
    })];
  });
  const labels = (Array.isArray(value.labels) ? value.labels : []).flatMap((label) => {
    if (!isRecord(label) || !Number.isSafeInteger(label.x) || !Number.isSafeInteger(label.y)) {
      return [];
    }
    return [Object.freeze({ x: label.x, y: label.y, text: stringValue(label.t) })];
  });
  return Object.freeze({
    location: stringValue(value.location),
    sheet: stringValue(value.sheet),
    rooms: Object.freeze(rooms),
    edges: Object.freeze(edges),
    labels: Object.freeze(labels),
  });
}

function normalizeMapState(value) {
  const state = isRecord(value) ? value : {};
  const normalizeGhosts = (entries) => Object.freeze(
    (Array.isArray(entries) ? entries : []).flatMap((ghost) => {
      if (!isRecord(ghost) || !Number.isSafeInteger(ghost.x) || !Number.isSafeInteger(ghost.y)) {
        return [];
      }
      return [Object.freeze({ x: ghost.x, y: ghost.y, current: ghost.cur === true })];
    }),
  );
  const ghostEdges = Object.freeze(
    (Array.isArray(state.ghost_edges) ? state.ghost_edges : []).flatMap((edge) => {
      if (!isRecord(edge)) return [];
      const coordinates = [edge.x1, edge.y1, edge.x2, edge.y2];
      if (!coordinates.every(Number.isSafeInteger)) return [];
      return [Object.freeze({
        x1: edge.x1,
        y1: edge.y1,
        x2: edge.x2,
        y2: edge.y2,
        label: nullableString(edge.l),
      })];
    }),
  );
  const rawTravel = isRecord(state.travel) ? state.travel : null;
  const travel = rawTravel && Number.isSafeInteger(rawTravel.dest)
    ? Object.freeze({
        destination: rawTravel.dest,
        done: finiteInteger(rawTravel.done),
        total: finiteInteger(rawTravel.total),
        eta: stringValue(rawTravel.eta),
      })
    : null;
  const rawClassic = isRecord(state.classic) ? state.classic : null;
  const classicRect = rawClassic ? finiteTuple(rawClassic.room_rect, 4) : null;
  const classic = rawClassic && typeof rawClassic.image === "string" && rawClassic.image && classicRect
    ? Object.freeze({ image: rawClassic.image, roomRect: classicRect })
    : null;
  return Object.freeze({
    available: state.available === true,
    location: nullableString(state.location),
    room: nullableInteger(state.room),
    cell: finiteTuple(state.cell, 2),
    classic,
    inGhost: state.in_ghost === true,
    ghosts: normalizeGhosts(state.ghosts),
    ghostEdges,
    travel,
  });
}

function normalizeStyledLine(value) {
  if (!isRecord(value)) return null;
  const segments = Array.isArray(value.segments)
    ? value.segments
        .filter(isRecord)
        .map((segment) => {
          const normalized = { ...segment };
          if (isRecord(normalized.link_data)) {
            normalized.link_data = Object.freeze({ ...normalized.link_data });
          }
          if (isRecord(normalized.inline_image)) {
            normalized.inline_image = Object.freeze({ ...normalized.inline_image });
          }
          return Object.freeze(normalized);
        })
    : [];
  return Object.freeze({ ...value, segments: Object.freeze(segments) });
}

function normalizeStyledLines(value) {
  return Object.freeze((Array.isArray(value) ? value : []).flatMap((line) => {
    const normalized = normalizeStyledLine(line);
    return normalized ? [normalized] : [];
  }));
}

function normalizeMenu(value) {
  if (!isRecord(value) || !Number.isSafeInteger(value.request_id) || value.request_id < 1) {
    return null;
  }
  const items = (Array.isArray(value.items) ? value.items : []).flatMap((item) => {
    if (!isRecord(item) || typeof item.text !== "string") return [];
    return [Object.freeze({
      text: item.text,
      command: stringValue(item.command),
      disabled: item.disabled === true,
    })];
  });
  return Object.freeze({
    requestId: value.request_id,
    noun: stringValue(value.noun),
    items: Object.freeze(items),
  });
}

function initialSlices() {
  return {
    session: null,
    character: null,
    availableStreams: Object.freeze([]),
    streams: Object.freeze(Object.create(null)),
    room: normalizeRoom(null),
    hands: normalizeHands(null),
    vitals: normalizeVitals(null),
    minivitals: Object.freeze([]),
    indicators: Object.freeze({}),
    timers: normalizeTimers(null),
    preparedSpell: null,
    entities: normalizeEntities(null),
    effects: Object.freeze([]),
    spellbook: Object.freeze([]),
    inventory: Object.freeze([]),
    injuries: Object.freeze({}),
    doll: normalizeDoll(null),
    targets: Object.freeze([]),
    field: Object.freeze([]),
    objectives: normalizeObjectives(null),
    charInfo: normalizeCharInfo(null),
    mapScene: null,
    mapState: normalizeMapState(null),
  };
}

function locationToken(location) {
  const hash = typeof location?.hash === "string" ? location.hash : "";
  const match = hash.match(/(?:^#|&)token=([^&]+)/i);
  if (!match) return "";
  try {
    return decodeURIComponent(match[1]);
  } catch {
    return match[1];
  }
}

function websocketUrl(location, explicitUrl) {
  if (explicitUrl) return String(explicitUrl);
  if (typeof location === "string") {
    const url = new URL(location);
    const protocol = url.protocol === "https:" ? "wss:" : "ws:";
    return `${protocol}//${url.host}/ws`;
  }
  const protocol = location?.protocol === "https:" ? "wss:" : "ws:";
  const host = location?.host;
  if (!host) throw new DesktopSessionError("location", "location.host is required");
  return `${protocol}//${host}/ws`;
}

/** An operational or protocol error exposed by DesktopSession. */
export class DesktopSessionError extends Error {
  constructor(code, message, options = {}) {
    super(message, options);
    this.name = "DesktopSessionError";
    this.code = code;
  }
}

/**
 * Owns one reconnecting WebSocket and the reduced initial Despana state slice.
 * `subscribe` returns an unsubscribe function and immediately reports the
 * current connection state. Listener exceptions never interrupt the session.
 */
export class DesktopSession {
  constructor(options = {}) {
    const nativeFactory = options.WebSocket
      ? (url) => new options.WebSocket(url)
      : typeof globalThis.WebSocket === "function"
        ? (url) => new globalThis.WebSocket(url)
        : null;
    this._webSocketFactory =
      options.webSocketFactory || options.createWebSocket || nativeFactory;
    if (typeof this._webSocketFactory !== "function") {
      throw new DesktopSessionError(
        "websocket-factory",
        "a WebSocket factory is required",
      );
    }

    this._location = options.location || globalThis.location;
    this._url = websocketUrl(this._location, options.url || options.webSocketUrl);
    this._storage = options.storage || null;
    this._tokenKey = options.tokenStorageKey || VELLUM_TOKEN_STORAGE_KEY;
    this._token =
      options.token || locationToken(this._location) || this._storageGet(this._tokenKey);
    if (options.token || locationToken(this._location)) {
      this._storageSet(this._tokenKey, this._token);
    }

    const timers = options.timers || {};
    this._setTimeout =
      timers.setTimeout || options.setTimeout || globalThis.setTimeout?.bind(globalThis);
    this._clearTimeout =
      timers.clearTimeout || options.clearTimeout || globalThis.clearTimeout?.bind(globalThis);
    if (typeof this._setTimeout !== "function" || typeof this._clearTimeout !== "function") {
      throw new DesktopSessionError("timers", "setTimeout and clearTimeout are required");
    }

    this._maxLinesPerStream = finiteInteger(
      options.maxLinesPerStream,
      DEFAULT_MAX_LINES_PER_STREAM,
    );
    if (this._maxLinesPerStream < 1) {
      this._maxLinesPerStream = DEFAULT_MAX_LINES_PER_STREAM;
    }

    this._listeners = new Set();
    this._socket = null;
    this._reconnectTimer = null;
    this._reconnectDelay = INITIAL_RECONNECT_MS;
    this._reconnectAttempt = 0;
    this._closed = false;
    this._fatal = false;
    this._authenticated = false;
    this._synchronized = false;
    this._epoch = null;
    this._lastTextSeq = 0;
    this._wireSeq = 0;
    this._revision = 0;
    this._dispatchId = 0;
    this._lastUnconfirmedDispatch = null;
    this._pendingMenuRequests = new Set();
    this._pendingMapRequests = new Map();
    this._state = Object.freeze({
      connection: Object.freeze({ status: "idle", attempt: 0, error: null }),
      ...initialSlices(),
    });
  }

  /** Open the transport. Repeated calls while open or connecting are no-ops. */
  connect() {
    if (this._closed) {
      throw new DesktopSessionError("closed", "the session is closed");
    }
    if (this._socket && (this._socket.readyState === 0 || this._socket.readyState === 1)) {
      return this;
    }

    this._cancelReconnect();
    this._setConnection(
      this._reconnectAttempt > 0 ? "reconnecting" : "connecting",
      null,
    );

    let socket;
    try {
      socket = this._webSocketFactory(this._url);
    } catch (error) {
      this._surfaceError("connect", error, true);
      this._scheduleReconnect();
      return this;
    }
    if (!socket) {
      this._surfaceError("connect", new Error("WebSocket factory returned no socket"), true);
      this._scheduleReconnect();
      return this;
    }

    this._socket = socket;
    this._authenticated = false;
    this._synchronized = false;
    socket.onopen = () => this._handleOpen(socket);
    socket.onmessage = (event) => this._handleRawMessage(socket, event?.data);
    socket.onerror = () => {
      if (this._socket !== socket || this._closed) return;
      this._surfaceError("transport", new Error("WebSocket transport error"), true);
    };
    socket.onclose = () => this._handleClose(socket);
    return this;
  }

  /** Subscribe to ordered reductions. Returns an idempotent unsubscribe. */
  subscribe(listener) {
    if (typeof listener !== "function") {
      throw new TypeError("listener must be a function");
    }
    this._listeners.add(listener);
    this._deliver(listener, this._event("connection", { connection: this._state.connection }));
    let subscribed = true;
    return () => {
      if (!subscribed) return;
      subscribed = false;
      this._listeners.delete(listener);
    };
  }

  /**
   * Send one explicit user command. The frame is sent once or the call throws;
   * it is never queued and therefore cannot be replayed after reconnect.
   */
  dispatch(intent) {
    const kind = intent?.kind || intent?.type;
    const socket = this._socket;
    if (!socket || socket.readyState !== 1 || !this._synchronized) {
      throw new DesktopSessionError("offline", "the session is not synchronized");
    }
    if (
      this._state.session?.session_control === true &&
      this._state.session.state !== "connected"
    ) {
      throw new DesktopSessionError("game-session", "the game session is not connected");
    }

    let frame;
    let id;
    let menuRequestId = null;
    let mapRequestKind = null;
    let unconfirmedDispatch = null;
    if (kind === "exit-and-logout") {
      id = `exit-${++this._dispatchId}`;
      frame = { t: "exit_logout", d: {} };
    } else if (kind === "submit-text" || kind === "command") {
      if (typeof intent.text !== "string" || intent.text.trim().length === 0) {
        throw new DesktopSessionError("intent", "command text must not be empty");
      }
      id = `command-${++this._dispatchId}`;
      frame = { t: "cmd", d: { text: intent.text } };
      unconfirmedDispatch = Object.freeze({
        id,
        kind: "command",
        label: intent.text,
      });
    } else if (kind === "link-tap") {
      const link = intent.link;
      if (
        !isRecord(link) ||
        !Number.isSafeInteger(intent.requestId) ||
        intent.requestId < 1 ||
        typeof link.exist_id !== "string" ||
        link.exist_id.length === 0 ||
        typeof link.noun !== "string"
      ) {
        throw new DesktopSessionError("intent", "link-tap intent is malformed");
      }
      id = `link-${intent.requestId}`;
      if (link.exist_id !== "_direct_" && !link.coord) {
        menuRequestId = intent.requestId;
      } else {
        unconfirmedDispatch = Object.freeze({
          id,
          kind: "action",
          label: link.text || link.noun || "game action",
        });
      }
      frame = {
        t: "link_tap",
        d: {
          request_id: intent.requestId,
          exist_id: link.exist_id,
          noun: link.noun,
          text: typeof link.text === "string" ? link.text : "",
          coord: typeof link.coord === "string" && link.coord ? link.coord : null,
        },
      };
    } else if (kind === "map-locations") {
      if (!Number.isSafeInteger(intent.requestId) || intent.requestId < 1) {
        throw new DesktopSessionError("intent", "map location request is malformed");
      }
      id = `map-locations-${intent.requestId}`;
      frame = { t: "map_locations", d: { request_id: intent.requestId } };
      mapRequestKind = "locations";
    } else if (kind === "map-view") {
      if (
        !Number.isSafeInteger(intent.requestId) ||
        intent.requestId < 1 ||
        typeof intent.location !== "string" ||
        !intent.location
      ) {
        throw new DesktopSessionError("intent", "map view request is malformed");
      }
      id = `map-view-${intent.requestId}`;
      frame = {
        t: "map_view",
        d: { request_id: intent.requestId, location: intent.location },
      };
      mapRequestKind = "view";
    } else {
      throw new DesktopSessionError("intent", "unsupported desktop intent");
    }

    try {
      socket.send(JSON.stringify(frame));
    } catch (error) {
      throw new DesktopSessionError("send", "intent was not sent", { cause: error });
    }
    // WebSocket.send() only proves that the browser accepted the frame. There
    // is no command acknowledgement in protocol v1, so keep the latest game
    // command/action available for an honest uncertainty signal if this
    // transport closes. A plain noun tap only requests a menu and is not yet a
    // game action; the eventual menu pick returns through submit-text.
    if (unconfirmedDispatch) this._lastUnconfirmedDispatch = unconfirmedDispatch;
    if (menuRequestId !== null) this._pendingMenuRequests.add(menuRequestId);
    if (mapRequestKind !== null) {
      this._pendingMapRequests.set(intent.requestId, mapRequestKind);
    }
    return Object.freeze({ id, status: "sent" });
  }

  /**
   * Adopt a pairing token entered in another same-origin Vellum surface.
   * Authentication denial is otherwise fatal by design, so explicitly
   * replacing the token is the one action that clears that terminal state
   * and starts a fresh handshake.
   */
  replacePairingToken(token) {
    if (typeof token !== "string" || token.trim().length === 0) {
      throw new DesktopSessionError("token", "pairing token must not be empty");
    }
    const next = token.trim();
    const changed = next !== this._token;
    this._token = next;
    this._storageSet(this._tokenKey, next);

    const status = this._state.connection.status;
    const restartHandshake = changed && (
      status === "connecting" || status === "authenticating"
    );
    if (status !== "denied" && !restartHandshake) return false;

    // A sibling Vellum page can publish the new token just before this socket
    // receives its own denial. Supersede an in-flight handshake as well as
    // recovering an already-denied one so that ordering race cannot strand
    // Despana in the fatal state.
    this._fatal = false;
    this._cancelReconnect();
    const socket = this._socket;
    this._socket = null;
    this._authenticated = false;
    this._synchronized = false;
    if (socket && (socket.readyState === 0 || socket.readyState === 1)) {
      try {
        socket.close();
      } catch {
        // The replacement handshake below is authoritative.
      }
    }
    this._reconnectAttempt = 0;
    this._reconnectDelay = INITIAL_RECONNECT_MS;
    this.connect();
    return true;
  }

  /** Permanently close this instance and cancel any scheduled reconnect. */
  close() {
    if (this._closed) return;
    this._closed = true;
    this._cancelReconnect();
    const socket = this._socket;
    this._socket = null;
    this._authenticated = false;
    this._synchronized = false;
    this._lastUnconfirmedDispatch = null;
    this._pendingMenuRequests.clear();
    this._pendingMapRequests.clear();
    if (socket && (socket.readyState === 0 || socket.readyState === 1)) {
      try {
        socket.close();
      } catch {
        // The observable close state is still authoritative for this adapter.
      }
    }
    this._setConnection("closed", null);
  }

  _handleOpen(socket) {
    if (this._socket !== socket || this._closed) return;
    this._setConnection("authenticating", null);
    try {
      socket.send(JSON.stringify({ t: "auth", d: { token: this._token || "" } }));
    } catch (error) {
      this._surfaceError("auth-send", error, true);
      try {
        socket.close();
      } catch {
        this._handleClose(socket);
      }
    }
  }

  _handleRawMessage(socket, raw) {
    if (this._socket !== socket || this._closed) return;
    let message;
    try {
      message = JSON.parse(typeof raw === "string" ? raw : String(raw));
    } catch (error) {
      this._surfaceError("malformed-json", error, true);
      return;
    }
    if (!isRecord(message) || message.v !== PROTOCOL_VERSION) {
      this._fatalProtocol(
        "protocol-version",
        `expected Vellum protocol v${PROTOCOL_VERSION}`,
        socket,
      );
      return;
    }
    if (typeof message.t !== "string" || !Number.isSafeInteger(message.seq) || message.seq < 0) {
      this._surfaceError("malformed-envelope", new Error("invalid protocol envelope"), true);
      return;
    }
    this._wireSeq = message.seq;

    if (message.t === "denied") {
      this._fatal = true;
      this._storageRemove(this._tokenKey);
      this._surfaceError("auth-denied", new Error("pairing token was denied"), false);
      this._setConnection("denied", "pairing token was denied");
      try {
        socket.close();
      } catch {
        this._socket = null;
      }
      return;
    }
    if (message.t === "hello") {
      this._handleHello(socket, message.d);
      return;
    }
    if (!this._authenticated) {
      this._surfaceError(
        "handshake-order",
        new Error("received data before hello"),
        true,
      );
      return;
    }
    if (message.t === "snapshot") {
      this._handleSnapshot(message.d);
      return;
    }
    if (!this._synchronized) {
      this._surfaceError(
        "handshake-order",
        new Error("received a delta before the initial snapshot"),
        true,
      );
      return;
    }
    this._handleDelta(message.t, message.seq, message.d);
  }

  _handleHello(socket, payload) {
    if (!isRecord(payload) || typeof payload.session !== "string" || !payload.session) {
      this._fatalProtocol("malformed-hello", "hello is missing its session epoch", socket);
      return;
    }

    const epochChanged = payload.session !== this._epoch;
    if (epochChanged) {
      this._epoch = payload.session;
      this._lastTextSeq = 0;
      this._replaceSlices({
        character: nullableString(payload.character),
        availableStreams: freezeArray(payload.streams),
      });
      this._emit("reset", { reason: "epoch" });
    } else {
      this._replaceState({
        character: nullableString(payload.character) || this._state.character,
        availableStreams: Array.isArray(payload.streams)
          ? freezeArray(payload.streams)
          : this._state.availableStreams,
      });
    }

    this._authenticated = true;
    this._setConnection("synchronizing", null);
    try {
      socket.send(JSON.stringify({ t: "subscribe", d: { mode: "desktop" } }));
      socket.send(JSON.stringify({ t: "resume", d: { seq: this._lastTextSeq } }));
    } catch (error) {
      this._surfaceError("handshake-send", error, true);
      try {
        socket.close();
      } catch {
        this._handleClose(socket);
      }
    }
  }

  _handleSnapshot(payload) {
    if (!isRecord(payload) || !["full", "resume", "gap"].includes(payload.mode)) {
      this._surfaceError("malformed-snapshot", new Error("invalid snapshot"), true);
      return;
    }

    const mode = payload.mode;
    // A snapshot is a synchronization barrier. In-flight menu and map
    // responses may have been among the deltas replaced by it and must never
    // surface later.
    this._pendingMenuRequests.clear();
    this._pendingMapRequests.clear();
    if (mode === "full") {
      this._lastTextSeq = 0;
      this._replaceSlices({ availableStreams: this._state.availableStreams });
    }

    const next = {
      session: normalizeSession(payload.session),
      character: nullableString(payload.character) || this._state.character,
      room: normalizeRoom(payload.room),
      hands: normalizeHands(payload.hands),
      vitals: normalizeVitals(payload.vitals),
      minivitals: normalizeMiniVitals(payload.minivitals),
      indicators: freezeRecord(payload.indicators),
      timers: normalizeTimers(payload.rt),
      preparedSpell: nullableString(payload.prepared_spell),
      entities: normalizeEntities(payload.entities),
      effects: normalizeEffects(payload.effects),
      spellbook: normalizeStyledLines(payload.spellbook),
      inventory: normalizeStyledLines(payload.inventory),
      injuries: normalizeInjuries(payload.injuries),
      doll: normalizeDoll({
        variant: payload.doll_variant,
        hidden: payload.doll_hidden,
      }),
      targets: normalizeTargets(payload.targets),
      field: normalizeField(payload.field),
      objectives: normalizeObjectives(payload.objectives),
      charInfo: normalizeCharInfo(payload.char_info),
      mapScene: normalizeMapScene(payload.map_scene),
      mapState: normalizeMapState(payload.map_state),
    };

    const streams = { ...this._state.streams };
    const acceptedText = [];
    for (const entry of Array.isArray(payload.text) ? payload.text : []) {
      const accepted = this._acceptText(entry?.seq, entry?.stream, entry?.line, streams);
      if (accepted) acceptedText.push(accepted);
    }
    next.streams = Object.freeze(streams);
    this._replaceState(next);
    this._synchronized = true;
    this._reconnectAttempt = 0;
    this._reconnectDelay = INITIAL_RECONNECT_MS;
    this._setConnection("connected", null, false);

    if (mode === "gap") {
      this._emit("gap", {
        marker: "missed-output",
        message: "Some output was evicted before the session could resume.",
      });
    }
    this._emit("snapshot", {
      mode,
      changed: Object.freeze([
        "session",
        "character",
        "streams",
        "room",
        "hands",
        "vitals",
        "minivitals",
        "indicators",
        "timers",
        "preparedSpell",
        "entities",
        "effects",
        "spellbook",
        "inventory",
        "injuries",
        "doll",
        "targets",
        "field",
        "objectives",
        "charInfo",
        "mapScene",
        "mapState",
      ]),
      text: Object.freeze(acceptedText),
    });
  }

  _handleDelta(type, seq, payload) {
    if (type === "text") {
      if (!isRecord(payload)) {
        this._surfaceError("malformed-text", new Error("invalid text delta"), true);
        return;
      }
      const streams = { ...this._state.streams };
      const accepted = this._acceptText(seq, payload.stream, payload.line, streams);
      if (!accepted) return;
      this._replaceState({ streams: Object.freeze(streams) });
      this._emit("text", accepted);
      return;
    }

    if (type === "menu") {
      const menu = normalizeMenu(payload);
      if (!menu) {
        this._surfaceError("malformed-menu", new Error("invalid menu delta"), true);
        return;
      }
      if (!this._pendingMenuRequests.delete(menu.requestId)) return;
      this._emit("menu", { menu });
      return;
    }

    if (type === "open_url") {
      const url = nullableString(payload?.url);
      if (!url || !/^https?:\/\//i.test(url)) {
        this._surfaceError("malformed-open-url", new Error("invalid external URL delta"), true);
        return;
      }
      this._emit("open-url", { url });
      return;
    }

    if (type === "map_locations") {
      const requestId = payload?.request_id;
      if (this._pendingMapRequests.get(requestId) !== "locations") return;
      this._pendingMapRequests.delete(requestId);
      // Vellum uses satellite keys internally to render uncurated current
      // rooms. They remain valid protocol data, but are not human-selectable
      // maps and therefore do not belong in Despana's browse picker.
      const locations = despanaMapLocations(payload?.locations);
      this._emit("map-locations", { requestId, locations });
      return;
    }

    if (type === "map_browse") {
      const requestId = payload?.request_id;
      if (this._pendingMapRequests.get(requestId) !== "view") return;
      this._pendingMapRequests.delete(requestId);
      this._emit("map-browse", {
        requestId,
        location: stringValue(payload?.location),
        scene: normalizeMapScene(payload?.scene),
        error: nullableString(payload?.error),
      });
      return;
    }

    const stateKey = TRACKED_STATE_DELTAS.get(type);
    if (!stateKey) return;
    let value;
    switch (type) {
      case "session":
        value = normalizeSession(payload);
        break;
      case "room":
        value = normalizeRoom(payload);
        break;
      case "hands":
        value = normalizeHands(payload);
        break;
      case "vitals":
        value = normalizeVitals(payload);
        break;
      case "minivitals":
        value = normalizeMiniVitals(payload);
        break;
      case "indicators":
        value = freezeRecord(payload);
        break;
      case "rt":
        value = normalizeTimers(payload);
        break;
      case "prepared_spell":
        value = nullableString(payload?.spell);
        break;
      case "entities":
        value = normalizeEntities(payload);
        break;
      case "effects":
        value = normalizeEffects(payload);
        break;
      case "spells":
        value = normalizeStyledLines(payload);
        break;
      case "inventory":
        value = normalizeStyledLines(payload);
        break;
      case "injuries":
        value = normalizeInjuries(payload);
        break;
      case "doll":
        value = normalizeDoll(payload);
        break;
      case "targets":
        value = normalizeTargets(payload);
        break;
      case "field":
        value = normalizeField(payload);
        break;
      case "objectives":
        value = normalizeObjectives(payload);
        break;
      case "charinfo":
        value = normalizeCharInfo(payload);
        break;
      case "map_scene":
        value = normalizeMapScene(payload);
        break;
      case "map_state":
        value = normalizeMapState(payload);
        break;
      default:
        return;
    }
    const changes = { [stateKey]: value };
    if (type === "session" && nullableString(payload?.character)) {
      changes.character = payload.character;
    }
    this._replaceState(changes);
    // State deltas deliberately do not consult lastTextSeq: several state
    // changes commonly share the newest text sequence number.
    this._emit("state", {
      changed: Object.freeze(
        changes.character ? [stateKey, "character"] : [stateKey],
      ),
    });
  }

  _acceptText(seq, stream, rawLine, streams) {
    if (!Number.isSafeInteger(seq) || seq < 0 || seq <= this._lastTextSeq) return null;
    if (typeof stream !== "string" || stream.length === 0) {
      this._surfaceError("malformed-text", new Error("text stream is missing"), true);
      return null;
    }
    const line = normalizeStyledLine(rawLine);
    if (!line) {
      this._surfaceError("malformed-text", new Error("styled line is malformed"), true);
      return null;
    }
    this._lastTextSeq = seq;
    const current = Array.isArray(streams[stream]) ? streams[stream] : [];
    const next = [...current, Object.freeze({ seq, stream, line })];
    if (next.length > this._maxLinesPerStream) {
      next.splice(0, next.length - this._maxLinesPerStream);
    }
    streams[stream] = Object.freeze(next);
    return Object.freeze({ seq, stream, line });
  }

  _replaceSlices(overrides = {}) {
    this._state = Object.freeze({
      connection: this._state.connection,
      ...initialSlices(),
      ...overrides,
    });
  }

  _replaceState(changes) {
    this._state = Object.freeze({ ...this._state, ...changes });
  }

  _handleClose(socket) {
    if (this._socket !== socket) return;
    this._socket = null;
    this._authenticated = false;
    this._synchronized = false;
    this._pendingMenuRequests.clear();
    this._pendingMapRequests.clear();
    if (this._closed) return;
    if (this._fatal) return;
    const uncertain = this._lastUnconfirmedDispatch;
    this._lastUnconfirmedDispatch = null;
    if (uncertain) {
      this._emit("dispatch-uncertain", {
        dispatch: uncertain,
        message: "The last command or action may not have reached the game and was not replayed.",
      });
    }
    this._setConnection("reconnecting", null);
    this._scheduleReconnect();
  }

  _scheduleReconnect() {
    if (this._closed || this._fatal || this._reconnectTimer !== null) return;
    const delay = this._reconnectDelay;
    this._reconnectAttempt += 1;
    this._replaceState({
      connection: Object.freeze({
        status: "reconnecting",
        attempt: this._reconnectAttempt,
        error: this._state.connection.error,
      }),
    });
    this._emit("connection", { connection: this._state.connection, delay });
    this._reconnectTimer = this._setTimeout(() => {
      this._reconnectTimer = null;
      if (!this._closed && !this._fatal) this.connect();
    }, delay);
    this._reconnectDelay = Math.min(delay * 2, MAX_RECONNECT_MS);
  }

  _cancelReconnect() {
    if (this._reconnectTimer === null) return;
    this._clearTimeout(this._reconnectTimer);
    this._reconnectTimer = null;
  }

  _setConnection(status, error = null, emit = true) {
    const connection = Object.freeze({
      status,
      attempt: this._reconnectAttempt,
      error,
    });
    this._replaceState({ connection });
    if (emit) this._emit("connection", { connection });
  }

  _fatalProtocol(code, message, socket) {
    this._fatal = true;
    this._surfaceError(code, new Error(message), false);
    this._setConnection("error", message);
    try {
      socket.close();
    } catch {
      this._socket = null;
    }
  }

  _surfaceError(code, error, recoverable) {
    const message = error instanceof Error ? error.message : String(error);
    this._emit("error", {
      error: Object.freeze({ code, message, recoverable }),
    });
  }

  _emit(type, detail = {}) {
    this._revision += 1;
    const event = this._event(type, detail);
    for (const listener of [...this._listeners]) this._deliver(listener, event);
  }

  _event(type, detail) {
    return Object.freeze({
      type,
      revision: this._revision,
      epoch: this._epoch,
      seq: this._wireSeq,
      textSeq: this._lastTextSeq,
      state: this._state,
      ...detail,
    });
  }

  _deliver(listener, event) {
    try {
      listener(event);
    } catch {
      // A faulty projection must not block other projections or the socket.
    }
  }

  _storageGet(key) {
    try {
      return this._storage?.getItem?.(key) || "";
    } catch {
      return "";
    }
  }

  _storageSet(key, value) {
    if (!value) return;
    try {
      this._storage?.setItem?.(key, value);
    } catch {
      // Private browsing and read-only storage are valid configurations.
    }
  }

  _storageRemove(key) {
    try {
      this._storage?.removeItem?.(key);
    } catch {
      // Storage failure must not change socket authentication behavior.
    }
  }
}

export default DesktopSession;
