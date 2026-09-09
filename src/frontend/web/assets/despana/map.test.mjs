import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = await readFile(new URL("./map.js", import.meta.url), "utf8");
const styles = await readFile(new URL("./app.css", import.meta.url), "utf8");
const moduleUrl = `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const {
  ClassicRoomStore,
  DesktopMapViewport,
  DesktopMapViewportError,
  classicRoomAtViewportPoint,
} = await import(moduleUrl);

// Issue #40: a rooms fetch that lands while the server's mapdb is loading
// used to be cached as "no rooms" forever, silently killing every classic
// map click for the rest of the session.
test("classic room store never caches an empty or failed answer", async () => {
  const answers = [[], [], [{ id: 7, rect: [0, 0, 10, 10] }]];
  let fetches = 0;
  const store = new ClassicRoomStore(async () => answers[fetches++]);

  assert.deepEqual(await store.load("landing.png"), []);
  assert.equal(store.get("landing.png"), null, "empty answer is not cached");
  assert.deepEqual(await store.load("landing.png"), []);
  assert.equal(fetches, 2, "each load retries while nothing usable is known");

  const rooms = await store.load("landing.png");
  assert.equal(rooms.length, 1);
  assert.equal(store.get("landing.png"), rooms);
  await store.load("landing.png");
  assert.equal(fetches, 3, "a non-empty answer is cached");

  const errors = [];
  const failing = new ClassicRoomStore(async () => { throw new Error("503"); });
  assert.deepEqual(await failing.load("briar.png", { onError: (e) => errors.push(e.message) }), []);
  assert.equal(failing.get("briar.png"), null);
  assert.deepEqual(errors, ["503"]);

  const aborted = new ClassicRoomStore(async () => {
    throw Object.assign(new Error("aborted"), { name: "AbortError" });
  });
  const abortErrors = [];
  await aborted.load("briar.png", { onError: (e) => abortErrors.push(e) });
  assert.deepEqual(abortErrors, [], "aborts are silent");
});

test("classic room store shares one in-flight request per image", async () => {
  let resolve = null;
  let fetches = 0;
  const store = new ClassicRoomStore((name) => {
    fetches += 1;
    return new Promise((r) => { resolve = () => r([{ id: 1, rect: [0, 0, 1, 1] }]); });
  });
  const first = store.load("a.png");
  const second = store.load("a.png");
  assert.equal(fetches, 1);
  resolve();
  assert.equal(await first, await second);
  assert.equal(store.has("a.png"), true);
  assert.deepEqual(await store.load(""), []);
  store.clear();
  assert.equal(store.get("a.png"), null);
});

const SCENE = Object.freeze({
  location: "Wehnimer's Landing",
  sheet: "outdoor",
  rooms: Object.freeze([{ i: 100, x: 10, y: 20 }]),
  edges: Object.freeze([]),
  labels: Object.freeze([]),
});

function frame(overrides = {}, scene = SCENE) {
  return {
    scene,
    state: {
      available: true,
      location: "Wehnimer's Landing",
      room: 100,
      cell: [10, 20],
      ...overrides,
    },
  };
}

test("normalizes a frame without copying its large scene arrays", () => {
  const model = new DesktopMapViewport();
  const snapshot = model.setFrame(frame());

  assert.equal(snapshot.scene.rooms, SCENE.rooms);
  assert.equal(snapshot.scene.edges, SCENE.edges);
  assert.deepEqual(snapshot.state.cell, [10, 20]);
  assert.equal(snapshot.state.inGhost, false);
  assert.deepEqual(snapshot.camera, { x: 10, y: 20, pixelsPerCell: 22 });
  assert.equal(snapshot.locationIdentity, "room:100");
  assert.ok(Object.isFrozen(snapshot));
  assert.ok(Object.isFrozen(snapshot.camera));
});

test("recenter follows room identity, not scene refreshes or occupant-like state", () => {
  const model = new DesktopMapViewport();
  model.setFrame(frame());
  model.beginDrag({ pointerId: 7, x: 100, y: 100 });
  model.dragTo({ pointerId: 7, x: 144, y: 78 });
  model.endDrag(7);
  assert.deepEqual(model.snapshot().camera, { x: 8, y: 21, pixelsPerCell: 22 });

  model.setFrame(frame({ travel: { dest: 200, done: 1, total: 3, eta: "0:12" } }));
  assert.deepEqual(model.snapshot().camera, { x: 8, y: 21, pixelsPerCell: 22 });

  const refreshedScene = { ...SCENE, labels: [{ x: 10, y: 20, t: "Square" }] };
  model.setFrame(frame({}, refreshedScene));
  assert.deepEqual(model.snapshot().camera, { x: 8, y: 21, pixelsPerCell: 22 });

  model.setFrame(frame({ room: 101, cell: [14, 25] }, refreshedScene));
  assert.deepEqual(model.snapshot().camera, { x: 14, y: 25, pixelsPerCell: 22 });
  assert.equal(model.snapshot().locationIdentity, "room:101");
});

test("a reset frame lets the same room identity recenter in a new session", () => {
  const model = new DesktopMapViewport();
  model.setFrame(frame());
  model.beginDrag({ x: 0, y: 0 });
  model.dragTo({ x: 44, y: 22 });
  model.endDrag();
  assert.deepEqual(model.snapshot().camera, { x: 8, y: 19, pixelsPerCell: 22 });

  model.setFrame({ scene: null, state: null });
  assert.equal(model.snapshot().locationIdentity, null);
  model.setFrame(frame());
  assert.deepEqual(model.snapshot().camera, { x: 10, y: 20, pixelsPerCell: 22 });
});

test("ghost movement keys recentering by location and cell despite a stale room id", () => {
  const model = new DesktopMapViewport();
  model.setFrame(frame({ in_ghost: true, cell: [3, 4] }));
  assert.deepEqual(model.snapshot().camera, { x: 3, y: 4, pixelsPerCell: 22 });

  model.beginDrag({ x: 0, y: 0 });
  model.dragTo({ x: 22, y: 22 });
  model.endDrag();
  assert.deepEqual(model.snapshot().camera, { x: 2, y: 3, pixelsPerCell: 22 });

  model.setFrame(frame({ inGhost: true, cell: [3, 4], travel: { done: 1 } }));
  assert.deepEqual(model.snapshot().camera, { x: 2, y: 3, pixelsPerCell: 22 });

  model.setFrame(frame({ inGhost: true, cell: [4, 4] }));
  assert.deepEqual(model.snapshot().camera, { x: 4, y: 4, pixelsPerCell: 22 });
  assert.equal(
    model.snapshot().locationIdentity,
    JSON.stringify(["cell", "Wehnimer's Landing", 4, 4]),
  );
});

test("Center restores the current cell and is a no-op without one", () => {
  const model = new DesktopMapViewport();
  assert.equal(model.center(), false);

  model.setFrame(frame());
  model.beginDrag({ x: 0, y: 0 });
  model.dragTo({ x: 44, y: -22 });
  model.endDrag();
  assert.equal(model.center(), true);
  assert.deepEqual(model.snapshot().camera, { x: 10, y: 20, pixelsPerCell: 22 });

  model.setFrame(frame({ cell: null }));
  assert.equal(model.center(), false);
});

test("dragging tracks one pointer and pans in cell units", () => {
  const model = new DesktopMapViewport({ pixelsPerCell: 10 });
  model.setFrame(frame({ cell: [0, 0] }));

  assert.equal(model.beginDrag({ pointerId: 1, x: 20, y: 30 }), true);
  assert.equal(model.snapshot().dragging, true);
  assert.equal(model.beginDrag({ pointerId: 2, x: 40, y: 50 }), false);
  assert.equal(model.dragTo({ pointerId: 2, x: 60, y: 60 }), false);
  assert.equal(model.dragTo({ pointerId: 1, x: 50, y: 10 }), true);
  assert.deepEqual(model.snapshot().camera, { x: -3, y: 2, pixelsPerCell: 10 });
  assert.equal(model.endDrag(2), false);
  assert.equal(model.endDrag(1), true);
  assert.equal(model.snapshot().dragging, false);
});

test("wheel zoom preserves the pointer's world cell and clamps scale", () => {
  const model = new DesktopMapViewport({
    pixelsPerCell: 20,
    minPixelsPerCell: 10,
    maxPixelsPerCell: 24,
  });
  model.setFrame(frame({ cell: [5, 7] }));

  const geometry = { x: 150, y: 25, width: 200, height: 100 };
  const before = model.snapshot().camera;
  const worldBefore = {
    x: before.x + (geometry.x - geometry.width / 2) / before.pixelsPerCell,
    y: before.y + (geometry.y - geometry.height / 2) / before.pixelsPerCell,
  };
  assert.equal(model.zoomWheel({ deltaY: -1, ...geometry }), true);

  const after = model.snapshot().camera;
  const worldAfter = {
    x: after.x + (geometry.x - geometry.width / 2) / after.pixelsPerCell,
    y: after.y + (geometry.y - geometry.height / 2) / after.pixelsPerCell,
  };
  assert.ok(Math.abs(worldAfter.x - worldBefore.x) < 1e-12);
  assert.ok(Math.abs(worldAfter.y - worldBefore.y) < 1e-12);

  for (let i = 0; i < 20; i += 1) model.zoomWheel({ deltaY: -1 });
  assert.equal(model.snapshot().camera.pixelsPerCell, 24);
  assert.equal(model.zoomWheel({ deltaY: -1 }), false);
  for (let i = 0; i < 40; i += 1) model.zoomWheel({ deltaY: 1 });
  assert.equal(model.snapshot().camera.pixelsPerCell, 10);
  assert.equal(model.zoomWheel({ deltaY: 1 }), false);
  assert.equal(model.zoomWheel({ deltaY: 0 }), false);
});

test("room hit testing uses the current camera and a bounded click target", () => {
  const model = new DesktopMapViewport({ pixelsPerCell: 20 });
  model.setFrame({
    scene: {
      ...SCENE,
      rooms: [
        { i: 100, x: 10, y: 20 },
        { i: 101, x: 12, y: 20 },
      ],
    },
    state: {
      available: true,
      location: "Wehnimer's Landing",
      room: 100,
      cell: [10, 20],
    },
  });

  assert.equal(model.roomAtViewportPoint({ x: 100, y: 50, width: 200, height: 100 }), 100);
  assert.equal(model.roomAtViewportPoint({ x: 140, y: 50, width: 200, height: 100 }), 101);
  assert.equal(model.roomAtViewportPoint({ x: 180, y: 50, width: 200, height: 100 }), null);

  model.beginDrag({ x: 0, y: 0 });
  model.dragTo({ x: -40, y: 0 });
  model.endDrag();
  assert.equal(model.roomAtViewportPoint({ x: 100, y: 50, width: 200, height: 100 }), 101);
  assert.equal(model.roomAtViewportPoint({ x: Number.NaN, y: 0, width: 1, height: 1 }), null);
});

test("classic-map hit testing resolves image rectangles after pan and zoom", () => {
  const rooms = [
    { id: 100, rect: [10, 20, 30, 40] },
    { id: 101, rect: [50, 20, 70, 40] },
  ];
  const camera = { x: 40, y: 30, scale: 2 };
  const viewport = { width: 200, height: 100 };

  assert.equal(classicRoomAtViewportPoint({
    x: 50,
    y: 50,
    ...viewport,
    camera,
    rooms,
  }), 100);
  assert.equal(classicRoomAtViewportPoint({
    x: 150,
    y: 50,
    ...viewport,
    camera,
    rooms,
  }), 101);
  assert.equal(classicRoomAtViewportPoint({
    x: 100,
    y: 5,
    ...viewport,
    camera,
    rooms,
  }), null);
});

test("rejects invalid scale configuration", () => {
  assert.throws(
    () => new DesktopMapViewport({ minPixelsPerCell: 20, maxPixelsPerCell: 10 }),
    (error) => error instanceof DesktopMapViewportError && error.code === "scale-range",
  );
  assert.throws(
    () => new DesktopMapViewport({ pixelsPerCell: Number.NaN }),
    (error) => error instanceof DesktopMapViewportError && error.code === "scale",
  );
});

test("classic mode CSS fully hides the local map canvas", () => {
  assert.match(
    styles,
    /#map-canvas\[hidden\]\s*\{[^}]*display:\s*none\s*;/s,
    "the explicit canvas display rule must not override its hidden attribute",
  );
});
