import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = await readFile(new URL("./session.js", import.meta.url), "utf8");
const moduleUrl = `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const {
  DesktopSession,
  DesktopSessionError,
  presentationTitle,
  shouldShowVellumIdle,
} = await import(moduleUrl);

const asterReplay = JSON.parse(await readFile(
  new URL("../../../../../tests/fixtures/despana/aster-ws-v1.json", import.meta.url),
  "utf8",
));
const briarReplay = JSON.parse(await readFile(
  new URL("../../../../../tests/fixtures/despana/briar-ws-v1.json", import.meta.url),
  "utf8",
));

class FakeStorage {
  constructor(entries = {}) {
    this.entries = new Map(Object.entries(entries));
  }

  getItem(key) {
    return this.entries.get(key) ?? null;
  }

  setItem(key, value) {
    this.entries.set(key, String(value));
  }

  removeItem(key) {
    this.entries.delete(key);
  }
}

class FakeTimers {
  constructor() {
    this.nextId = 1;
    this.pending = new Map();
    this.delays = [];
  }

  setTimeout(fn, delay) {
    const id = this.nextId++;
    this.pending.set(id, fn);
    this.delays.push(delay);
    return id;
  }

  clearTimeout(id) {
    this.pending.delete(id);
  }

  runNext() {
    const entry = this.pending.entries().next().value;
    assert.ok(entry, "expected a scheduled timer");
    const [id, fn] = entry;
    this.pending.delete(id);
    fn();
  }
}

class FakeWebSocket {
  constructor(url) {
    this.url = url;
    this.readyState = 0;
    this.sent = [];
    this.onopen = null;
    this.onmessage = null;
    this.onerror = null;
    this.onclose = null;
  }

  open() {
    this.readyState = 1;
    this.onopen?.({});
  }

  receive(message) {
    this.onmessage?.({ data: JSON.stringify(message) });
  }

  serverClose() {
    this.readyState = 3;
    this.onclose?.({ code: 1006 });
  }

  send(raw) {
    assert.equal(this.readyState, 1, "client may only send on an open socket");
    this.sent.push(JSON.parse(raw));
  }

  close() {
    this.readyState = 3;
    this.onclose?.({ code: 1000 });
  }
}

function frame(t, d, seq = 0) {
  return { v: 1, seq, t, d };
}

function line(text, stream = "main") {
  return {
    stream,
    segments: [{ text, fg: "#c9b37e", bg: null, bold: false }],
  };
}

function fullSnapshot(overrides = {}) {
  return {
    mode: "full",
    character: "Briar",
    session: {
      state: "connected",
      character: "Briar",
      game: "GS3",
      session_control: true,
    },
    room: {
      name: "Town Square Central",
      id: "100",
      exits: ["n", "e"],
      description: [line("A busy square.")],
    },
    hands: { left: "a crystal orb", right: null },
    vitals: { health: 95, mana: 84, stamina: 73, spirit: 100 },
    minivitals: [
      { id: "health", value: 190, max: 200 },
      { id: "mana", value: 84, max: 100 },
    ],
    indicators: { standing: true, hidden: false },
    rt: { roundtime_end: 120, casttime_end: 118, server_time: 100 },
    prepared_spell: "Elemental Saturation (413)",
    entities: {
      creatures: [{ id: "creature-1", label: "a giant rat", noun: "rat" }],
      objects: [{ id: "object-1", label: "a silver coin", noun: "coin" }],
      players: [{ id: "player-1", label: "Aster", noun: "Aster" }],
    },
    effects: [{
      category: "ActiveSpells",
      effects: [{
        id: "509",
        text: "Strength of the Bull",
        value: 92,
        time: "00:24:10",
        expires_at: 1540,
        bar_color: "#a86f32",
        text_color: null,
      }],
      generation: 1,
    }],
    spellbook: [line("Elemental Defense III (503)", "Spells")],
    inventory: [{
      stream: "inv",
      segments: [{
        text: "a patchwork backpack",
        fg: "#c9b37e",
        bg: null,
        bold: false,
        link_data: {
          exist_id: "535703780",
          noun: "backpack",
          text: "patchwork backpack",
          coord: null,
        },
      }],
    }],
    injuries: { head: 2 },
    doll_variant: "runic",
    doll_hidden: ["leftLeg"],
    targets: [{
      id: "#creature-1",
      name: "a giant rat",
      noun: "rat",
      status: "stunned",
      current: true,
    }],
    field: [{
      id: "creature-1",
      noun: "rat",
      name: "a giant rat",
      rect: [1, 2, 101, 82],
      foot: [51, 82],
      dead: false,
      boss: false,
      current: true,
      statuses: ["stunned"],
      lift: null,
    }],
    objectives: {
      objectives: [{
        id: "24352",
        kind: "QUEST",
        state: "available",
        name: "Into the Rift",
        description: "Assist an adventurer.",
        location: "The Rift",
        cadence: "monthly",
        rewards: [{ reward_type: "experience", amount: 5000 }],
        actions: [{ action_type: "accept", cmd: "QUEST ACCEPT s24352" }],
      }],
      generation: 3,
    },
    char_info: {
      profession: "Sorcerer",
      level: 90,
      experience: ["Mind: muddled (42%)"],
      encumbrance: ["Light (17%)"],
      bounty: ["Cull 10 rats"],
      society: ["Order of Voln"],
      gauges: {
        mind: { value: 42, text: "muddled" },
        encumbrance: { value: 17, text: "Light" },
        stance: { value: 80, text: "defensive" },
        field_exp: { value: 500, max: 1000 },
      },
    },
    map_scene: {
      location: "Wehnimer's Landing",
      sheet: "outdoor",
      rooms: [{ i: 100, x: 0, y: 0, e: true }],
      edges: [{ x1: 0, y1: 0, x2: 1, y2: 0, k: 0, l: "east", ar: 100, br: 101 }],
      labels: [{ x: 0, y: -1, t: "Town Square" }],
    },
    map_state: {
      available: true,
      location: "Wehnimer's Landing",
      room: 100,
      cell: [0, 0],
      classic: {
        image: "wl-wehnimers-1264234799.png",
        room_rect: [277, 615, 313, 651],
      },
      in_ghost: false,
      ghosts: [{ x: 2, y: 1, cur: true }],
      ghost_edges: [{ x1: 1, y1: 0, x2: 2, y2: 1, l: "path" }],
      travel: { dest: 101, done: 1, total: 2, eta: "0:05" },
    },
    text: [{ seq: 1, stream: "main", line: line("Welcome, Briar.") }],
    ...overrides,
  };
}

function makeHarness(options = {}) {
  const sockets = [];
  const timers = new FakeTimers();
  const storage = options.storage || new FakeStorage({ "vellum-token": "stored-token" });
  const events = [];
  const session = new DesktopSession({
    location: { protocol: "http:", host: "127.0.0.1:8040", hash: "" },
    storage,
    webSocketFactory(url) {
      const socket = new FakeWebSocket(url);
      sockets.push(socket);
      return socket;
    },
    timers: {
      setTimeout: timers.setTimeout.bind(timers),
      clearTimeout: timers.clearTimeout.bind(timers),
    },
    ...options.sessionOptions,
  });
  session.subscribe((event) => events.push(event));
  return { session, sockets, timers, storage, events };
}

function connectAndHello(harness, epoch = "epoch-1", character = "Briar") {
  harness.session.connect();
  const socket = harness.sockets.at(-1);
  socket.open();
  socket.receive(
    frame(
      "hello",
      { character, streams: ["main", "thoughts"], session: epoch },
      1,
    ),
  );
  return socket;
}

function openForReplay(harness) {
  harness.session.connect();
  const socket = harness.sockets.at(-1);
  socket.open();
  return socket;
}

test("the native idle handoff owns controlled non-running states but not reconnects", () => {
  assert.equal(shouldShowVellumIdle({ connection: { status: "connecting" } }), true);
  assert.equal(shouldShowVellumIdle({
    connection: { status: "denied" },
    session: { state: "connected", session_control: true },
  }), true, "pairing denial must remain recoverable even with retained session state");

  for (const state of ["idle", "authenticating", "connecting", "disconnected"]) {
    assert.equal(shouldShowVellumIdle({
      connection: { status: "connected" },
      session: { state, session_control: true },
    }), true, `${state} must show the native idle handoff`);
  }

  for (const state of ["connected", "reconnecting"]) {
    assert.equal(shouldShowVellumIdle({
      connection: { status: "reconnecting" },
      session: { state, session_control: true },
    }), false, `${state} must keep Despana visible`);
  }

  assert.equal(shouldShowVellumIdle({
    connection: { status: "reconnecting" },
    session: { state: "connected", session_control: false },
  }), false, "sidecar transports do not have a Vellum login lifecycle");
});

test("presentation title uses confirmed identity fields with graceful fallbacks", () => {
  assert.equal(presentationTitle({}), "Vellum Despana");
  assert.equal(
    presentationTitle({ session: { character: "Briar" } }),
    "Vellum Despana - Briar",
  );
  assert.equal(
    presentationTitle({
      character: "Briar",
      charInfo: { profession: "Sorcerer", level: 90 },
    }),
    "Vellum Despana - Briar - Sorcerer - 90",
  );
  assert.equal(
    presentationTitle({ character: "Briar", charInfo: { level: 0 } }),
    "Vellum Despana - Briar - 0",
  );
});

test("a token entered in a separate Vellum page recovers a denied desktop session", () => {
  const harness = makeHarness({
    storage: new FakeStorage({ "vellum-token": "stale-token" }),
  });
  harness.session.connect();
  const deniedSocket = harness.sockets.at(-1);
  deniedSocket.open();
  assert.deepEqual(deniedSocket.sent, [
    { t: "auth", d: { token: "stale-token" } },
  ]);

  deniedSocket.receive(frame("denied", {}, 1));
  assert.equal(harness.events.at(-1).connection.status, "denied");
  assert.equal(harness.storage.getItem("vellum-token"), null);

  assert.equal(harness.session.replacePairingToken(" fresh-token "), true);
  assert.equal(harness.storage.getItem("vellum-token"), "fresh-token");
  const recoveredSocket = harness.sockets.at(-1);
  assert.notEqual(recoveredSocket, deniedSocket);
  recoveredSocket.open();
  assert.deepEqual(recoveredSocket.sent, [
    { t: "auth", d: { token: "fresh-token" } },
  ]);
  assert.equal(harness.events.at(-1).connection.status, "authenticating");
});

test("a changed Vellum token supersedes an in-flight desktop handshake", () => {
  const harness = makeHarness({
    storage: new FakeStorage({ "vellum-token": "stale-token" }),
  });
  harness.session.connect();
  const staleSocket = harness.sockets.at(-1);
  staleSocket.open();

  assert.equal(harness.session.replacePairingToken("fresh-token"), true);
  assert.equal(staleSocket.readyState, 3);
  const freshSocket = harness.sockets.at(-1);
  freshSocket.open();
  assert.deepEqual(freshSocket.sent, [
    { t: "auth", d: { token: "fresh-token" } },
  ]);
  assert.equal(harness.timers.pending.size, 0);
});

function receiveReplayFrame(socket, message) {
  assert.equal(message?.v, 1, "fixture frame must use WebSocket protocol v1");
  assert.equal(typeof message?.t, "string", "fixture frame must have a type");
  assert.ok(Number.isSafeInteger(message?.seq), "fixture frame must have a sequence");
  assert.ok(Object.hasOwn(message, "d"), "fixture frame must have a payload");
  socket.receive(message);
}

function expectedHandshake() {
  return [
    { t: "auth", d: { token: "stored-token" } },
    { t: "subscribe", d: { mode: "play" } },
    { t: "resume", d: { seq: 0 } },
  ];
}

const replayScenarios = [
  {
    name: "Aster",
    frames: asterReplay,
    room: { id: "3717", name: "Darkstone, Winding Tunnel" },
    marker: "ASTER_REPLAY_MARKER: ",
    inventory: {
      text: "a torn scroll",
      existId: "fixture-aster-scroll-01",
      noun: "scroll",
    },
    effect: { id: "101", text: "Spirit Warding I" },
    target: {
      id: "#fixture-aster-target-01",
      linkId: "fixture-aster-target-01",
      name: "a massive troll king",
      status: "stunned",
    },
    objective: {
      id: "fixture-aster-task-01",
      name: "Cull the troll kings",
      state: "active",
    },
    map: { location: "Darkstone Castle", room: 3717, cell: [4, 2] },
  },
  {
    name: "Briar",
    frames: briarReplay,
    room: { id: "23780", name: "Duskruin Arena, Sands Approach" },
    marker: "BRIAR_REPLAY_MARKER: ",
    inventory: {
      text: "a polished oak runestaff",
      existId: "fixture-briar-staff-01",
      noun: "runestaff",
    },
    effect: { id: "425", text: "Elemental Targeting" },
    target: {
      id: "#fixture-briar-target-01",
      linkId: "fixture-briar-target-01",
      name: "an arena champion",
      status: null,
    },
    objective: {
      id: "fixture-briar-task-01",
      name: "Complete an arena match",
      state: "complete",
    },
    map: { location: "Duskruin Arena", room: 23780, cell: [8, 6] },
  },
];

function assertReplayState(harness, socket, scenario) {
  const event = harness.events.findLast((entry) => entry.state);
  assert.ok(event, `${scenario.name} replay must emit synchronized state`);
  assert.equal(
    harness.events.some((entry) => entry.type === "error"),
    false,
    `${scenario.name} replay must not emit protocol errors`,
  );

  const { state } = event;
  assert.equal(state.character, scenario.name);
  assert.equal(state.session.character, scenario.name);
  assert.equal(state.room.id, scenario.room.id);
  assert.equal(state.room.name, scenario.room.name);

  const storySegments = state.streams.main.flatMap((entry) => entry.line.segments);
  assert.equal(
    storySegments.some((segment) => segment.text === scenario.marker),
    true,
  );
  const storyLink = storySegments.find((segment) => segment.link_data);
  assert.equal(storyLink.link_data.exist_id, scenario.target.linkId);
  assert.equal(storyLink.link_data.noun, scenario.target.name.split(" ").at(-1));

  assert.equal(state.inventory.length, 1);
  assert.equal(state.inventory[0].segments[0].text, scenario.inventory.text);
  assert.deepEqual(state.inventory[0].segments[0].link_data, {
    exist_id: scenario.inventory.existId,
    noun: scenario.inventory.noun,
    text: scenario.inventory.text.replace(/^(?:a|an|some) /, ""),
    coord: null,
  });
  assert.equal(state.effects[0].effects[0].id, scenario.effect.id);
  assert.equal(state.effects[0].effects[0].text, scenario.effect.text);
  assert.equal(state.targets[0].id, scenario.target.id);
  assert.equal(state.targets[0].name, scenario.target.name);
  assert.equal(state.targets[0].status, scenario.target.status);
  assert.equal(state.objectives.objectives[0].id, scenario.objective.id);
  assert.equal(state.objectives.objectives[0].name, scenario.objective.name);
  assert.equal(state.objectives.objectives[0].state, scenario.objective.state);
  assert.equal(state.mapScene.location, scenario.map.location);
  assert.equal(state.mapState.location, scenario.map.location);
  assert.equal(state.mapState.room, scenario.map.room);
  assert.deepEqual(state.mapState.cell, scenario.map.cell);

  assert.ok(Object.isFrozen(state.inventory[0].segments[0].link_data));
  assert.ok(Object.isFrozen(storyLink.link_data));
  assert.deepEqual(socket.sent, expectedHandshake());
  assert.equal(socket.sent.some((message) => ["cmd", "link_tap"].includes(message.t)), false);
  return state;
}

for (const scenario of replayScenarios) {
  test(`sanitized ${scenario.name} protocol replay reduces the desktop contract`, () => {
    const harness = makeHarness();
    const socket = openForReplay(harness);
    for (const message of scenario.frames) receiveReplayFrame(socket, message);

    assertReplayState(harness, socket, scenario);
  });
}

test("interleaved Aster and Briar replays remain session-isolated", () => {
  const aster = makeHarness();
  const briar = makeHarness();
  const asterSocket = openForReplay(aster);
  const briarSocket = openForReplay(briar);
  const frameCount = Math.max(asterReplay.length, briarReplay.length);

  for (let index = 0; index < frameCount; index += 1) {
    if (asterReplay[index]) receiveReplayFrame(asterSocket, asterReplay[index]);
    if (briarReplay[index]) receiveReplayFrame(briarSocket, briarReplay[index]);
  }

  const asterState = assertReplayState(aster, asterSocket, replayScenarios[0]);
  const briarState = assertReplayState(briar, briarSocket, replayScenarios[1]);
  assert.notStrictEqual(asterState, briarState);
  assert.notStrictEqual(asterState.streams, briarState.streams);
  assert.notStrictEqual(asterState.inventory, briarState.inventory);
  assert.equal(
    asterState.streams.main.some((entry) => entry.line.segments.some(
      (segment) => segment.text === "BRIAR_REPLAY_MARKER: ",
    )),
    false,
  );
  assert.equal(
    briarState.streams.main.some((entry) => entry.line.segments.some(
      (segment) => segment.text === "ASTER_REPLAY_MARKER: ",
    )),
    false,
  );
  assert.equal(
    asterState.inventory[0].segments[0].link_data.exist_id,
    "fixture-aster-scroll-01",
  );
  assert.equal(
    briarState.inventory[0].segments[0].link_data.exist_id,
    "fixture-briar-staff-01",
  );
});

test("auth is first, then play subscription, then resume", () => {
  const harness = makeHarness();
  harness.session.connect();
  const socket = harness.sockets[0];

  assert.equal(socket.url, "ws://127.0.0.1:8040/ws");
  assert.deepEqual(socket.sent, []);
  socket.open();
  assert.deepEqual(socket.sent, [
    { t: "auth", d: { token: "stored-token" } },
  ]);

  socket.receive(
    frame(
      "hello",
      { character: "Briar", streams: ["main"], session: "epoch-1" },
      4,
    ),
  );
  assert.deepEqual(socket.sent, [
    { t: "auth", d: { token: "stored-token" } },
    { t: "subscribe", d: { mode: "play" } },
    { t: "resume", d: { seq: 0 } },
  ]);
});

test("a full snapshot reduces the complete initial desktop slice", () => {
  const harness = makeHarness();
  const socket = connectAndHello(harness);
  socket.receive(frame("snapshot", fullSnapshot(), 1));

  const event = harness.events.findLast((entry) => entry.type === "snapshot");
  assert.ok(event);
  assert.equal(event.mode, "full");
  assert.equal(event.state.connection.status, "connected");
  assert.equal(event.state.session.state, "connected");
  assert.equal(event.state.character, "Briar");
  assert.deepEqual(event.state.availableStreams, ["main", "thoughts"]);
  assert.equal(event.state.room.name, "Town Square Central");
  assert.equal(event.state.room.id, "100");
  assert.deepEqual(event.state.room.exits, ["n", "e"]);
  assert.equal(event.state.hands.left, "a crystal orb");
  assert.equal(event.state.hands.right, null);
  assert.deepEqual(event.state.vitals, {
    health: 95,
    mana: 84,
    stamina: 73,
    spirit: 100,
  });
  assert.deepEqual(event.state.minivitals[0], {
    id: "health",
    value: 190,
    max: 200,
  });
  assert.equal(event.state.indicators.standing, true);
  assert.deepEqual(event.state.timers, {
    roundtimeEnd: 120,
    casttimeEnd: 118,
    serverTime: 100,
  });
  assert.equal(event.state.preparedSpell, "Elemental Saturation (413)");
  assert.deepEqual(event.state.entities, {
    creatures: [{ id: "creature-1", label: "a giant rat", noun: "rat" }],
    objects: [{ id: "object-1", label: "a silver coin", noun: "coin" }],
    players: [{ id: "player-1", label: "Aster", noun: "Aster" }],
  });
  assert.equal(event.state.effects[0].category, "ActiveSpells");
  assert.deepEqual(event.state.effects[0].effects[0], {
    id: "509",
    text: "Strength of the Bull",
    value: 92,
    time: "00:24:10",
    expiresAt: 1540,
    barColor: "#a86f32",
    textColor: null,
  });
  assert.equal(event.state.spellbook[0].segments[0].text, "Elemental Defense III (503)");
  assert.equal(event.state.inventory[0].segments[0].text, "a patchwork backpack");
  assert.equal(
    event.state.inventory[0].segments[0].link_data.exist_id,
    "535703780",
  );
  assert.deepEqual(event.state.injuries, { head: 2 });
  assert.deepEqual(event.state.doll, { variant: "runic", hidden: ["leftLeg"] });
  assert.equal(event.state.targets[0].current, true);
  assert.deepEqual(event.state.field[0].rect, [1, 2, 101, 82]);
  assert.equal(event.state.objectives.objectives[0].actions[0].command, "QUEST ACCEPT s24352");
  assert.equal(event.state.charInfo.gauges.stance.text, "defensive");
  assert.equal(event.state.charInfo.gauges.fieldExp.max, 1000);
  assert.equal(event.state.charInfo.profession, "Sorcerer");
  assert.equal(event.state.charInfo.level, 90);
  assert.deepEqual(event.state.mapScene.rooms[0], {
    i: 100,
    x: 0,
    y: 0,
    entrance: true,
  });
  assert.equal(event.state.mapState.travel.destination, 101);
  assert.equal(event.state.mapState.ghosts[0].current, true);
  assert.ok(Object.isFrozen(event.state.effects[0].effects[0]));
  assert.ok(Object.isFrozen(event.state.inventory));
  assert.ok(Object.isFrozen(event.state.inventory[0].segments[0].link_data));
  assert.ok(Object.isFrozen(event.state.objectives.objectives[0].actions));
  assert.ok(Object.isFrozen(event.state.mapScene.rooms));
  assert.ok(Object.isFrozen(event.state.mapState.travel));
  assert.equal(event.state.streams.main.length, 1);
  assert.equal(event.state.streams.main[0].line.segments[0].text, "Welcome, Briar.");
});

test("text de-duplicates by seq while state at the same seq still applies", () => {
  const harness = makeHarness();
  const socket = connectAndHello(harness);
  socket.receive(frame("snapshot", fullSnapshot(), 1));
  socket.receive(frame("text", { stream: "main", line: line("Second line") }, 2));
  socket.receive(frame("text", { stream: "main", line: line("Duplicate") }, 2));
  socket.receive(
    frame("vitals", { health: 41, mana: 42, stamina: 43, spirit: 44 }, 2),
  );

  const event = harness.events.at(-1);
  assert.equal(event.type, "state");
  assert.equal(event.seq, 2);
  assert.equal(event.textSeq, 2);
  assert.equal(event.state.streams.main.length, 2);
  assert.equal(event.state.streams.main[1].line.segments[0].text, "Second line");
  assert.equal(event.state.vitals.health, 41);
});

test("an entities delta replaces the normalized entity projection", () => {
  const harness = makeHarness();
  const socket = connectAndHello(harness);
  socket.receive(frame("snapshot", fullSnapshot(), 1));
  socket.receive(
    frame(
      "entities",
      {
        creatures: [
          { id: "creature-2", label: "a cave troll", noun: "troll", ignored: true },
          { label: "missing id", noun: "stranger" },
        ],
        objects: [{ id: "object-2", label: 42, noun: "chest" }],
        players: null,
      },
      2,
    ),
  );

  const event = harness.events.at(-1);
  assert.equal(event.type, "state");
  assert.deepEqual(event.changed, ["entities"]);
  assert.deepEqual(event.state.entities, {
    creatures: [{ id: "creature-2", label: "a cave troll", noun: "troll" }],
    objects: [{ id: "object-2", label: "", noun: "chest" }],
    players: [],
  });
});

test("parity deltas replace frozen slices even when they share one sequence", () => {
  const harness = makeHarness();
  const socket = connectAndHello(harness);
  socket.receive(frame("snapshot", fullSnapshot(), 1));

  const deltas = [
    ["effects", [], "effects"],
    ["spells", [line("Minor Water (903)", "Spells")], "spellbook"],
    ["inventory", [line("a crystal orb", "inv")], "inventory"],
    ["injuries", { rightArm: 6, healthy: 0, impossible: 9 }, "injuries"],
    ["doll", { variant: null, hidden: ["rightArm", 42] }, "doll"],
    ["targets", [{
      id: "#creature-2",
      name: "a cave troll",
      noun: "troll",
      status: null,
      current: false,
    }], "targets"],
    ["field", [{
      id: "creature-2",
      noun: "troll",
      name: "a cave troll",
      rect: [2, 3, 82, 93],
      foot: [42, 93],
      dead: false,
      boss: true,
      current: false,
      statuses: ["prone"],
      lift: -0.5,
    }], "field"],
    ["objectives", { objectives: [], generation: 4 }, "objectives"],
    ["charinfo", {
      profession: "Ranger",
      level: 42,
      experience: ["Mind: clear (0%)"],
      gauges: { mind: { value: 0, text: "clear" } },
    }, "charInfo"],
    ["map_scene", {
      location: "The Rift",
      sheet: "interiors",
      rooms: [{ i: 500, x: -2, y: 4, e: false }],
      edges: [],
      labels: [],
    }, "mapScene"],
    ["map_state", {
      available: true,
      location: "The Rift",
      room: 500,
      cell: [-2, 4],
      in_ghost: true,
      ghosts: [],
      ghost_edges: [],
    }, "mapState"],
  ];

  const before = harness.events.length;
  for (const [type, payload] of deltas) socket.receive(frame(type, payload, 2));
  const events = harness.events.slice(before);

  assert.deepEqual(events.map((event) => event.changed), deltas.map(([, , key]) => [key]));
  assert.ok(events.every((event) => event.type === "state"));
  assert.ok(events.every((event) => event.seq === 2 && event.textSeq === 1));
  const view = events.at(-1).state;
  assert.deepEqual(view.effects, []);
  assert.equal(view.spellbook[0].segments[0].text, "Minor Water (903)");
  assert.equal(view.inventory[0].segments[0].text, "a crystal orb");
  assert.deepEqual(view.injuries, { rightArm: 6 });
  assert.deepEqual(view.doll, { variant: null, hidden: ["rightArm"] });
  assert.equal(view.targets[0].id, "#creature-2");
  assert.equal(view.field[0].boss, true);
  assert.deepEqual(view.objectives, { objectives: [], generation: 4 });
  assert.equal(view.charInfo.gauges.mind.text, "clear");
  assert.equal(view.charInfo.profession, "Ranger");
  assert.equal(view.charInfo.level, 42);
  assert.equal(view.mapScene.rooms[0].i, 500);
  assert.equal(view.mapState.inGhost, true);
  assert.equal(view.mapState.travel, null);
  assert.equal(view.mapState.classic, null);
  assert.ok(Object.isFrozen(view));
  assert.ok(Object.isFrozen(view.spellbook[0].segments[0]));
  assert.ok(Object.isFrozen(view.field[0].statuses));
  assert.ok(Object.isFrozen(view.charInfo.gauges));
  assert.ok(Object.isFrozen(view.mapState.ghostEdges));
});

test("a replacement snapshot clears omitted parity projections", () => {
  const harness = makeHarness();
  const socket = connectAndHello(harness);
  socket.receive(frame("snapshot", fullSnapshot(), 1));

  const replacement = fullSnapshot({ text: [] });
  for (const key of [
    "effects",
    "spellbook",
    "inventory",
    "injuries",
    "doll_variant",
    "doll_hidden",
    "targets",
    "field",
    "objectives",
    "char_info",
    "map_scene",
    "map_state",
  ]) {
    delete replacement[key];
  }
  socket.receive(frame("snapshot", replacement, 2));

  const view = harness.events.at(-1).state;
  assert.deepEqual(view.effects, []);
  assert.deepEqual(view.spellbook, []);
  assert.deepEqual(view.inventory, []);
  assert.deepEqual(view.injuries, {});
  assert.deepEqual(view.doll, { variant: null, hidden: [] });
  assert.deepEqual(view.targets, []);
  assert.deepEqual(view.field, []);
  assert.deepEqual(view.objectives, { objectives: [], generation: 0 });
  assert.deepEqual(view.charInfo, {
    profession: null,
    level: null,
    experience: [],
    encumbrance: [],
    bounty: [],
    society: [],
    gauges: { mind: null, encumbrance: null, stance: null, fieldExp: null },
  });
  assert.equal(view.mapScene, null);
  assert.deepEqual(view.mapState, {
    available: false,
    location: null,
    room: null,
    cell: null,
    classic: null,
    inGhost: false,
    ghosts: [],
    ghostEdges: [],
    travel: null,
  });
});

test("classic map metadata and local map browse replies stay structured", () => {
  const harness = makeHarness();
  const socket = connectAndHello(harness);
  socket.receive(frame("snapshot", fullSnapshot(), 1));

  assert.deepEqual(harness.events.at(-1).state.mapState.classic, {
    image: "wl-wehnimers-1264234799.png",
    roomRect: [277, 615, 313, 651],
  });

  harness.session.dispatch({ kind: "map-locations", requestId: 31 });
  assert.deepEqual(socket.sent.at(-1), {
    t: "map_locations",
    d: { request_id: 31 },
  });
  socket.receive(frame("map_locations", {
    request_id: 31,
    locations: ["Wehnimer's Landing", "Darkstone Castle"],
  }, 2));
  const locations = harness.events.at(-1);
  assert.equal(locations.type, "map-locations");
  assert.deepEqual(locations.locations, ["Wehnimer's Landing", "Darkstone Castle"]);

  harness.session.dispatch({ kind: "map-view", requestId: 32, location: "Darkstone Castle" });
  assert.deepEqual(socket.sent.at(-1), {
    t: "map_view",
    d: { request_id: 32, location: "Darkstone Castle" },
  });
  socket.receive(frame("map_browse", {
    request_id: 32,
    location: "Darkstone Castle",
    scene: {
      location: "Darkstone Castle",
      sheet: "outdoor",
      rooms: [{ i: 3717, x: 4, y: 8, e: false }],
      edges: [],
      labels: [],
    },
    error: null,
  }, 3));
  const browse = harness.events.at(-1);
  assert.equal(browse.type, "map-browse");
  assert.equal(browse.location, "Darkstone Castle");
  assert.equal(browse.scene.rooms[0].i, 3717);
});

test("a synchronization snapshot invalidates stale map browse replies", () => {
  const harness = makeHarness();
  const socket = connectAndHello(harness);
  socket.receive(frame("snapshot", fullSnapshot(), 1));

  harness.session.dispatch({ kind: "map-locations", requestId: 41 });
  harness.session.dispatch({ kind: "map-view", requestId: 42, location: "Darkstone Castle" });
  socket.receive(frame("snapshot", fullSnapshot(), 2));

  socket.receive(frame("map_locations", {
    request_id: 41,
    locations: ["stale location"],
  }, 3));
  socket.receive(frame("map_browse", {
    request_id: 42,
    location: "Darkstone Castle",
    scene: {
      location: "Darkstone Castle",
      sheet: "outdoor",
      rooms: [{ i: 3717, x: 4, y: 8, e: false }],
      edges: [],
      labels: [],
    },
    error: null,
  }, 4));

  assert.equal(harness.events.some((event) => event.type === "map-locations"), false);
  assert.equal(harness.events.some((event) => event.type === "map-browse"), false);
});

test("a link tap dispatches the exact protocol v1 wire frame", () => {
  const harness = makeHarness();
  const socket = connectAndHello(harness);
  socket.receive(frame("snapshot", fullSnapshot(), 1));

  const receipt = harness.session.dispatch({
    kind: "link-tap",
    requestId: 17,
    link: {
      exist_id: "creature-1",
      noun: "rat",
      text: "giant rat",
      coord: "42,19",
    },
  });

  assert.deepEqual(receipt, { id: "link-17", status: "sent" });
  assert.deepEqual(socket.sent.at(-1), {
    t: "link_tap",
    d: {
      request_id: 17,
      exist_id: "creature-1",
      noun: "rat",
      text: "giant rat",
      coord: "42,19",
    },
  });
});

test("a live combat target dispatch preserves its protocol-prefixed id", () => {
  const harness = makeHarness();
  const socket = connectAndHello(harness);
  socket.receive(frame("snapshot", fullSnapshot(), 1));

  const receipt = harness.session.dispatch({
    kind: "link-tap",
    requestId: 18,
    link: {
      exist_id: "#209691632",
      noun: "king",
      text: "a massive troll king",
      coord: null,
    },
  });

  assert.deepEqual(receipt, { id: "link-18", status: "sent" });
  assert.deepEqual(socket.sent.at(-1), {
    t: "link_tap",
    d: {
      request_id: 18,
      exist_id: "#209691632",
      noun: "king",
      text: "a massive troll king",
      coord: null,
    },
  });
});

test("noun menus are correlated transient events and are emitted only once", () => {
  const harness = makeHarness();
  const socket = connectAndHello(harness);
  socket.receive(frame("snapshot", fullSnapshot(), 1));
  const synchronizedState = harness.events.at(-1).state;

  socket.receive(frame("menu", {
    request_id: 17,
    noun: "rat",
    items: [{ text: "Look", command: "look at rat", disabled: false }],
  }, 2));
  assert.equal(harness.events.some((event) => event.type === "menu"), false);

  harness.session.dispatch({
    kind: "link-tap",
    requestId: 17,
    link: { exist_id: "creature-1", noun: "rat", text: "giant rat" },
  });
  const reply = {
    request_id: 17,
    noun: "rat",
    items: [
      { text: "Actions", command: "", disabled: true },
      { text: "Look", command: "look at rat", disabled: false },
    ],
  };
  socket.receive(frame("menu", reply, 2));
  socket.receive(frame("menu", reply, 2));

  const menuEvents = harness.events.filter((event) => event.type === "menu");
  assert.equal(menuEvents.length, 1);
  assert.deepEqual(menuEvents[0].menu, {
    requestId: 17,
    noun: "rat",
    items: [
      { text: "Actions", command: "", disabled: true },
      { text: "Look", command: "look at rat", disabled: false },
    ],
  });
  assert.equal(menuEvents[0].state, synchronizedState);
  assert.equal("menu" in menuEvents[0].state, false);
  assert.ok(Object.isFrozen(menuEvents[0].menu));
  assert.ok(Object.isFrozen(menuEvents[0].menu.items));
  assert.ok(Object.isFrozen(menuEvents[0].menu.items[0]));
});

test("all intents honor controlled game-session state while sidecars stay playable", () => {
  const controlled = makeHarness();
  const controlledSocket = connectAndHello(controlled);
  controlledSocket.receive(
    frame("snapshot", fullSnapshot({
      session: { state: "idle", character: "Briar", session_control: true },
    }), 1),
  );

  assert.throws(
    () => controlled.session.dispatch({ kind: "submit-text", text: "look" }),
    (error) => error instanceof DesktopSessionError && error.code === "game-session",
  );
  assert.throws(
    () => controlled.session.dispatch({
      kind: "link-tap",
      requestId: 2,
      link: { exist_id: "creature-1", noun: "rat", text: "giant rat" },
    }),
    (error) => error instanceof DesktopSessionError && error.code === "game-session",
  );

  const sidecar = makeHarness();
  const sidecarSocket = connectAndHello(sidecar);
  sidecarSocket.receive(
    frame("snapshot", fullSnapshot({
      session: { state: "idle", character: "Briar", session_control: false },
    }), 1),
  );
  assert.equal(
    sidecar.session.dispatch({ kind: "submit-text", text: "look" }).status,
    "sent",
  );
});

test("a changed server epoch clears stale state and resumes from zero", () => {
  const harness = makeHarness();
  const first = connectAndHello(harness, "epoch-1", "Briar");
  first.receive(frame("snapshot", fullSnapshot(), 1));
  first.serverClose();
  harness.timers.runNext();

  const second = harness.sockets.at(-1);
  second.open();
  second.receive(
    frame(
      "hello",
      { character: "Aster", streams: ["main"], session: "epoch-2" },
      20,
    ),
  );

  const reset = harness.events.findLast((entry) => entry.type === "reset");
  assert.ok(reset);
  assert.equal(reset.reason, "epoch");
  assert.equal(reset.epoch, "epoch-2");
  assert.equal(reset.textSeq, 0);
  assert.equal(reset.state.character, "Aster");
  assert.equal(reset.state.room.name, null);
  assert.deepEqual(reset.state.effects, []);
  assert.deepEqual(reset.state.spellbook, []);
  assert.deepEqual(reset.state.inventory, []);
  assert.deepEqual(reset.state.injuries, {});
  assert.deepEqual(reset.state.doll, { variant: null, hidden: [] });
  assert.deepEqual(reset.state.targets, []);
  assert.deepEqual(reset.state.field, []);
  assert.deepEqual(reset.state.objectives, { objectives: [], generation: 0 });
  assert.equal(reset.state.charInfo.gauges.mind, null);
  assert.equal(reset.state.mapScene, null);
  assert.equal(reset.state.mapState.available, false);
  assert.deepEqual(Object.keys(reset.state.streams), []);
  assert.deepEqual(second.sent.at(-1), { t: "resume", d: { seq: 0 } });
});

test("a gap snapshot surfaces a visible gap event and retained tail", () => {
  const harness = makeHarness();
  const socket = connectAndHello(harness);
  socket.receive(
    frame(
      "snapshot",
      fullSnapshot({
        mode: "gap",
        text: [{ seq: 12, stream: "main", line: line("Retained tail") }],
      }),
      12,
    ),
  );

  const gapIndex = harness.events.findIndex((entry) => entry.type === "gap");
  const snapshotIndex = harness.events.findIndex(
    (entry, index) => index > gapIndex && entry.type === "snapshot",
  );
  assert.ok(gapIndex >= 0);
  assert.ok(snapshotIndex > gapIndex);
  assert.equal(harness.events[gapIndex].marker, "missed-output");
  assert.equal(harness.events[snapshotIndex].state.streams.main[0].seq, 12);
});

test("reconnect resumes state but never resends a dispatched command", () => {
  const harness = makeHarness();
  const first = connectAndHello(harness);
  first.receive(frame("snapshot", fullSnapshot(), 1));

  const receipt = harness.session.dispatch({ kind: "submit-text", text: "look" });
  assert.equal(receipt.status, "sent");
  assert.equal(first.sent.filter((message) => message.t === "cmd").length, 1);

  first.serverClose();
  const uncertain = harness.events.findLast(
    (event) => event.type === "dispatch-uncertain",
  );
  assert.ok(uncertain, "disconnect must surface uncertain command delivery");
  assert.deepEqual(uncertain.dispatch, {
    id: "command-1",
    kind: "command",
    label: "look",
  });
  assert.match(uncertain.message, /may not have reached the game/i);
  assert.match(uncertain.message, /not replayed/i);
  assert.equal(harness.events.at(-1).type, "connection");
  assert.equal(harness.events.at(-1).connection.status, "reconnecting");
  assert.deepEqual(harness.timers.delays, [1000]);
  assert.throws(
    () => harness.session.dispatch({ kind: "submit-text", text: "look" }),
    (error) => error instanceof DesktopSessionError && error.code === "offline",
  );

  harness.timers.runNext();
  const second = harness.sockets.at(-1);
  second.open();
  second.receive(
    frame(
      "hello",
      { character: "Briar", streams: ["main"], session: "epoch-1" },
      2,
    ),
  );
  assert.deepEqual(second.sent, [
    { t: "auth", d: { token: "stored-token" } },
    { t: "subscribe", d: { mode: "play" } },
    { t: "resume", d: { seq: 1 } },
  ]);

  second.receive(
    frame(
      "snapshot",
      fullSnapshot({ mode: "resume", text: [] }),
      2,
    ),
  );
  assert.equal(second.sent.some((message) => message.t === "cmd"), false);
});

test("a disconnect without a dispatched command or action emits no uncertainty", () => {
  const harness = makeHarness();
  const socket = connectAndHello(harness);
  socket.receive(frame("snapshot", fullSnapshot(), 1));

  socket.serverClose();

  assert.equal(
    harness.events.some((event) => event.type === "dispatch-uncertain"),
    false,
  );
});

test("a direct link action reports uncertain delivery but a noun-menu request does not", () => {
  const direct = makeHarness();
  const directSocket = connectAndHello(direct);
  directSocket.receive(frame("snapshot", fullSnapshot(), 1));
  direct.session.dispatch({
    kind: "link-tap",
    requestId: 4,
    link: { exist_id: "_direct_", noun: "north", text: "go north" },
  });
  directSocket.serverClose();
  assert.deepEqual(
    direct.events.findLast((event) => event.type === "dispatch-uncertain")?.dispatch,
    { id: "link-4", kind: "action", label: "go north" },
  );

  const menu = makeHarness();
  const menuSocket = connectAndHello(menu);
  menuSocket.receive(frame("snapshot", fullSnapshot(), 1));
  menu.session.dispatch({
    kind: "link-tap",
    requestId: 5,
    link: { exist_id: "creature-1", noun: "rat", text: "giant rat" },
  });
  menuSocket.serverClose();
  assert.equal(
    menu.events.some((event) => event.type === "dispatch-uncertain"),
    false,
  );
});

test("reconnect backoff escalates after failure and resets after synchronization", () => {
  const harness = makeHarness();
  const first = connectAndHello(harness);
  first.receive(frame("snapshot", fullSnapshot(), 1));

  first.serverClose();
  assert.deepEqual(harness.timers.delays, [1000]);
  harness.timers.runNext();

  const second = harness.sockets.at(-1);
  second.serverClose();
  assert.deepEqual(harness.timers.delays, [1000, 2000]);
  harness.timers.runNext();

  const third = harness.sockets.at(-1);
  third.open();
  third.receive(
    frame(
      "hello",
      { character: "Briar", streams: ["main"], session: "epoch-1" },
      2,
    ),
  );
  third.receive(
    frame(
      "snapshot",
      fullSnapshot({ mode: "resume", text: [] }),
      2,
    ),
  );
  third.serverClose();

  assert.deepEqual(harness.timers.delays, [1000, 2000, 1000]);
});
