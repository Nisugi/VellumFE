// Probe for the flagged issue in session.js _handleDelta (~lines 1074-1091):
// map_locations / map_browse deltas read `payload?.request_id` and use it as
// a Map key without validating it, unlike every other normalized field.
// Question: can a malformed request_id (object, huge float, null, string,
// fractional number) cause a throw, a wrong lookup, or is it a harmless miss?
//
// Pending map request keys are always safe integers >= 1: dispatch() enforces
// `Number.isSafeInteger(intent.requestId) && intent.requestId >= 1` before
// `this._pendingMapRequests.set(intent.requestId, kind)`. Map.get uses
// SameValueZero, so any non-identical value (a string "31", an object, null,
// 31.0000001, a huge float) simply misses and the delta is ignored.

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = await readFile(new URL("./session.js", import.meta.url), "utf8");
const moduleUrl = `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const { DesktopSession } = await import(moduleUrl);

class FakeStorage {
  constructor(entries = {}) { this.entries = new Map(Object.entries(entries)); }
  getItem(key) { return this.entries.get(key) ?? null; }
  setItem(key, value) { this.entries.set(key, String(value)); }
  removeItem(key) { this.entries.delete(key); }
}

class FakeWebSocket {
  constructor(url) {
    this.url = url;
    this.readyState = 0;
    this.sent = [];
    this.onopen = null; this.onmessage = null; this.onerror = null; this.onclose = null;
  }
  open() { this.readyState = 1; this.onopen?.({}); }
  receive(message) { this.onmessage?.({ data: JSON.stringify(message) }); }
  receiveRaw(data) { this.onmessage?.({ data }); }
  send(raw) { this.sent.push(JSON.parse(raw)); }
  close() { this.readyState = 3; this.onclose?.({ code: 1000 }); }
}

function frame(t, d, seq = 0) { return { v: 1, seq, t, d }; }

function minimalSnapshot() {
  return { mode: "full", character: "Briar", session: { state: "connected", character: "Briar", game: "GS3", session_control: true } };
}

function makeHarness() {
  const sockets = [];
  const events = [];
  const session = new DesktopSession({
    location: { protocol: "http:", host: "127.0.0.1:8040", hash: "" },
    storage: new FakeStorage({ "vellum-token": "stored-token" }),
    webSocketFactory(url) {
      const socket = new FakeWebSocket(url);
      sockets.push(socket);
      return socket;
    },
    timers: { setTimeout: () => 1, clearTimeout: () => {} },
  });
  session.subscribe((event) => events.push(event));
  return { session, sockets, events };
}

function boot(harness) {
  harness.session.connect();
  const socket = harness.sockets.at(-1);
  socket.open();
  socket.receive(frame("hello", { character: "Briar", streams: ["main"], session: "epoch-1" }, 1));
  socket.receive(frame("snapshot", minimalSnapshot(), 1));
  return socket;
}

const PATHOLOGICAL_IDS = [
  ["object", { evil: true }],
  ["array", [31]],
  ["null", null],
  ["string of the pending id", "31"],
  ["huge float", 1e300],
  ["fractional near-miss", 31.0000000001],
  ["negative", -31],
  ["zero", 0],
  ["beyond safe integer", 2 ** 53 + 2],
];

test("pathological map_locations request_id values never throw, never emit, never clear the pending request", () => {
  const harness = makeHarness();
  const socket = boot(harness);

  harness.session.dispatch({ kind: "map-locations", requestId: 31 });

  let seq = 2;
  for (const [label, id] of PATHOLOGICAL_IDS) {
    assert.doesNotThrow(
      () => socket.receive(frame("map_locations", { request_id: id, locations: ["Evil Town"] }, seq++)),
      `request_id as ${label} must not throw`,
    );
  }
  // JSON cannot carry NaN/Infinity/undefined; exercise them via a raw frame.
  socket.receiveRaw('{"v":1,"seq":98,"t":"map_locations","d":{"locations":["Evil Town"]}}');
  assert.equal(
    harness.events.some((event) => event.type === "map-locations"),
    false,
    "no pathological request_id may surface a map-locations event",
  );

  // The genuine reply still lands: the pending entry was not consumed.
  socket.receive(frame("map_locations", { request_id: 31, locations: ["Wehnimer's Landing"] }, 99));
  const event = harness.events.at(-1);
  assert.equal(event.type, "map-locations");
  assert.equal(event.requestId, 31);
  assert.deepEqual(event.locations, ["Wehnimer's Landing"]);
});

test("pathological map_browse request_id values are equally inert", () => {
  const harness = makeHarness();
  const socket = boot(harness);

  harness.session.dispatch({ kind: "map-view", requestId: 32, location: "Darkstone Castle" });

  let seq = 2;
  for (const [label, id] of PATHOLOGICAL_IDS) {
    assert.doesNotThrow(
      () => socket.receive(frame("map_browse", {
        request_id: id,
        location: "Darkstone Castle",
        scene: null,
        error: "spoofed",
      }, seq++)),
      `request_id as ${label} must not throw`,
    );
  }
  assert.equal(harness.events.some((event) => event.type === "map-browse"), false);

  // A map_locations reply must not consume a pending map-view request even
  // with a matching id: the kind check ("view" vs "locations") gates it.
  socket.receive(frame("map_locations", { request_id: 32, locations: ["Evil Town"] }, 97));
  assert.equal(harness.events.some((event) => event.type === "map-locations"), false);

  socket.receive(frame("map_browse", {
    request_id: 32,
    location: "Darkstone Castle",
    scene: { location: "Darkstone Castle", sheet: "outdoor", rooms: [], edges: [], labels: [] },
    error: null,
  }, 99));
  const event = harness.events.at(-1);
  assert.equal(event.type, "map-browse");
  assert.equal(event.requestId, 32);
  assert.equal(event.location, "Darkstone Castle");
});
