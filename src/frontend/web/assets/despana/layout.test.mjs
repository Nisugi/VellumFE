import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = await readFile(new URL("./layout.js", import.meta.url), "utf8");
const moduleUrl = `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const {
  DEFAULT_DESPANA_LAYOUT,
  LAYOUT_ZONES,
  WorkspaceLayout,
  WorkspaceLayoutError,
  defaultDespanaModuleIds,
  layoutStorageKey,
  normalizeLayoutCharacter,
} = await import(moduleUrl);

const MODULE_IDS = defaultDespanaModuleIds();

function restore(saved = null, options = {}) {
  return WorkspaceLayout.restore({
    moduleIds: options.moduleIds || MODULE_IDS,
    defaults: options.defaults || DEFAULT_DESPANA_LAYOUT,
    saved,
    character: options.character || "Aster",
  });
}

function locations(snapshot) {
  const result = [];
  for (const zone of LAYOUT_ZONES) {
    for (const entry of snapshot.zones[zone].modules) {
      result.push({ id: entry.id, kind: "zone", zone });
    }
  }
  for (const entry of snapshot.hidden) {
    result.push({ id: entry.id, kind: "hidden", zone: entry.zone });
  }
  return result;
}

function assertCanonical(snapshot, expectedIds = MODULE_IDS) {
  const placed = locations(snapshot).map((entry) => entry.id);
  assert.equal(placed.length, expectedIds.length);
  assert.deepEqual([...placed].sort(), [...expectedIds].sort());
  assert.equal(new Set(placed).size, expectedIds.length);
  for (const zone of LAYOUT_ZONES) {
    const modules = snapshot.zones[zone].modules;
    if (!modules.length) continue;
    assert.equal(
      modules.reduce((total, entry) => total + entry.weight, 0),
      1000,
    );
    assert.ok(modules.every((entry) => entry.weight > 0));
  }
}

function find(snapshot, id) {
  return locations(snapshot).find((entry) => entry.id === id);
}

test("default restore is canonical and deeply frozen", () => {
  const model = restore();
  const snapshot = model.snapshot();

  assert.equal(snapshot.version, 1);
  assert.equal(snapshot.character, "aster");
  assertCanonical(snapshot);
  assert.equal(MODULE_IDS.includes("command"), false);
  assert.ok(Object.isFrozen(snapshot));
  assert.ok(Object.isFrozen(snapshot.zones));
  assert.ok(Object.isFrozen(snapshot.zones.center.modules));
  assert.throws(() => snapshot.zones.center.modules.push({ id: "nope", weight: 1 }));
});

test("factory workspace matches the proven Calvix starting layout", () => {
  const snapshot = restore().snapshot();

  assert.deepEqual(snapshot.tracks, {
    top: 181,
    bottom: 79,
    left: 288,
    right: 388,
  });
  assert.deepEqual(
    Object.fromEntries(
      LAYOUT_ZONES.map((zone) => [
        zone,
        snapshot.zones[zone].modules.map(({ id, weight }) => ({ id, weight })),
      ]),
    ),
    {
      top: [
        { id: "thoughts", weight: 473 },
        { id: "familiar", weight: 527 },
      ],
      bottom: [
        { id: "hands", weight: 166 },
        { id: "conditions", weight: 293 },
        { id: "cooldowns", weight: 274 },
        { id: "injuries", weight: 267 },
      ],
      left: [
        { id: "active-spells", weight: 550 },
        { id: "combat", weight: 223 },
        { id: "vitals", weight: 227 },
      ],
      right: [
        { id: "map", weight: 627 },
        { id: "tasks", weight: 373 },
      ],
      center: [
        { id: "room", weight: 225 },
        { id: "story", weight: 775 },
      ],
    },
  );
  assert.deepEqual(
    snapshot.hidden.map(({ id, zone, weight }) => ({ id, zone, weight })),
    [
      { id: "compass", zone: "right", weight: 110 },
      { id: "inventory", zone: "right", weight: 125 },
      { id: "known-spells", zone: "right", weight: 78 },
    ],
  );
});

test("canonical workspace weights remain stable across repeated restore cycles", () => {
  let serialized = restore().serialize();
  for (let cycle = 0; cycle < 10; cycle += 1) {
    const model = WorkspaceLayout.restore({
      moduleIds: MODULE_IDS,
      defaults: DEFAULT_DESPANA_LAYOUT,
      saved: serialized,
      character: "Aster",
    });
    assert.equal(model.serialize(), serialized);
    serialized = model.serialize();
  }
});

test("move, reorder, and flow intents preserve every module exactly once", () => {
  const model = restore();

  model.apply({ type: "move", id: "story", zone: "top", index: 1 });
  assert.deepEqual(
    model.snapshot().zones.top.modules.map((entry) => entry.id),
    ["thoughts", "story", "familiar"],
  );
  assert.equal(find(model.snapshot(), "story").zone, "top");

  model.apply({ type: "move", id: "hands", zone: "top", index: 2 });
  assert.deepEqual(
    model.snapshot().zones.top.modules.map((entry) => entry.id),
    ["thoughts", "story", "hands", "familiar"],
  );
  model.apply({ type: "set-flow", zone: "top", flow: "horizontal" });
  assert.equal(model.snapshot().zones.top.flow, "horizontal");
  assertCanonical(model.snapshot());
});

test("hide and show retain the previous zone, index, and a positive weight", () => {
  const model = restore();
  const original = structuredClone(model.snapshot().zones.center.modules);
  const before = model.snapshot().zones.center.modules[1];

  model.apply({ type: "hide", id: "story" });
  const hidden = model.snapshot().hidden.find((entry) => entry.id === "story");
  assert.deepEqual(hidden, {
    id: "story",
    zone: "center",
    index: 1,
    weight: before.weight,
    before: "room",
    after: null,
    order: ["room", "story"],
  });
  assert.equal(model.snapshot().zones.center.modules[0].weight, 1000);

  model.apply({ type: "show", id: "story" });
  assert.deepEqual(
    model.snapshot().zones.center.modules.map((entry) => entry.id),
    ["room", "story"],
  );
  assert.deepEqual(model.snapshot().zones.center.modules, original);
  assert.equal(model.snapshot().hidden.some((entry) => entry.id === "story"), false);
  assertCanonical(model.snapshot());
});

test("two-module zones restore their stable order and proportional sizes", () => {
  const orders = [
    ["room", "story"],
    ["story", "room"],
  ];
  for (const hideOrder of orders) {
    for (const showOrder of orders) {
      const model = restore();
      const original = structuredClone(model.snapshot().zones.center.modules);
      for (const id of hideOrder) model.apply({ type: "hide", id });
      assert.equal(model.snapshot().zones.center.modules.length, 0);
      for (const id of showOrder) model.apply({ type: "show", id });
      assert.deepEqual(
        model.snapshot().zones.center.modules,
        original,
        `hide ${hideOrder.join(", ")}; show ${showOrder.join(", ")}`,
      );
      assertCanonical(model.snapshot());
    }
  }
});

test("legacy version-one command placements are discarded", () => {
  const saved = JSON.parse(restore().serialize());
  saved.zones.bottom.modules = [
    { id: "conditions", weight: 250 },
    { id: "command", weight: 450 },
    { id: "vitals", weight: 300 },
  ];

  const snapshot = restore(saved).snapshot();

  assert.equal(find(snapshot, "command"), undefined);
  assert.deepEqual(
    snapshot.zones.bottom.modules.map((entry) => entry.id),
    ["hands", "conditions", "cooldowns", "injuries", "vitals"],
  );
  assertCanonical(snapshot);
});

test("resize-pair preserves pair total and clamps both modules positive", () => {
  const model = restore();
  const before = model.snapshot().zones.center.modules;
  const pairTotal = before[0].weight + before[1].weight;

  model.apply({
    type: "resize-pair",
    zone: "center",
    before: "room",
    after: "story",
    delta: 10_000,
  });
  const after = model.snapshot().zones.center.modules;
  assert.equal(after[0].weight + after[1].weight, pairTotal);
  assert.equal(after[1].weight, 1);
  assert.ok(after[0].weight > 0);
  assertCanonical(model.snapshot());
});

test("resize-track accepts only dock tracks and clamps persisted pixels", () => {
  const model = restore();
  model.apply({ type: "resize-track", zone: "left", pixels: 2 });
  assert.equal(model.snapshot().tracks.left, 48);
  model.apply({ type: "resize-track", zone: "right", pixels: 100_000 });
  assert.equal(model.snapshot().tracks.right, 4096);
  assert.throws(
    () => model.apply({ type: "resize-track", zone: "center", pixels: 200 }),
    (error) => error instanceof WorkspaceLayoutError && error.code === "track-zone",
  );
});

test("corrupt saved fields are sanitized and unknown or duplicate modules disappear", () => {
  const saved = {
    version: 1,
    character: "ASTER",
    tracks: { top: -50, bottom: 180, left: "wide", right: Infinity },
    zones: {
      top: {
        flow: "diagonal",
        modules: [
          { id: "story", weight: -7 },
          { id: "not-shipped", weight: 500 },
          { id: "story", weight: 300 },
        ],
      },
      center: { flow: "horizontal", modules: [{ id: "room", weight: 0 }] },
    },
    hidden: [
      { id: "thoughts", zone: "right", index: 3, weight: 222 },
      { id: "room", zone: "left", index: 0, weight: 100 },
    ],
  };
  const snapshot = restore(saved).snapshot();

  assertCanonical(snapshot);
  assert.equal(snapshot.zones.top.flow, DEFAULT_DESPANA_LAYOUT.zones.top.flow);
  assert.equal(snapshot.zones.center.flow, "horizontal");
  assert.equal(snapshot.tracks.bottom, 180);
  assert.equal(snapshot.tracks.left, DEFAULT_DESPANA_LAYOUT.tracks.left);
  assert.equal(snapshot.tracks.right, DEFAULT_DESPANA_LAYOUT.tracks.right);
  assert.equal(find(snapshot, "not-shipped"), undefined);
  assert.deepEqual(snapshot.hidden.find((entry) => entry.id === "thoughts"), {
    id: "thoughts",
    zone: "right",
    index: 3,
    weight: 222,
    before: null,
    after: null,
    order: null,
  });
});

test("newly shipped default modules merge into old version-one saves", () => {
  const upgradedDefault = JSON.parse(JSON.stringify(DEFAULT_DESPANA_LAYOUT));
  upgradedDefault.zones.left.modules.push({ id: "recent-loot", weight: 250 });
  const upgradedIds = [...MODULE_IDS, "recent-loot"];
  const oldSaved = JSON.parse(restore().serialize());

  const upgraded = restore(oldSaved, {
    defaults: upgradedDefault,
    moduleIds: upgradedIds,
  }).snapshot();
  assertCanonical(upgraded, upgradedIds);
  assert.equal(find(upgraded, "recent-loot").zone, "left");
});

test("wrong versions, malformed JSON, and character mismatches fall back atomically", () => {
  for (const saved of [
    "{not json",
    { version: 2, character: "aster", zones: {} },
    { version: 1, character: "Briar", zones: {} },
  ]) {
    const snapshot = restore(saved).snapshot();
    assertCanonical(snapshot);
    assert.deepEqual(
      snapshot.zones.center.modules.map((entry) => entry.id),
      ["room", "story"],
    );
  }
});

test("invalid intents are transactional", () => {
  const model = restore();
  const before = model.snapshot();
  assert.throws(
    () => model.apply({ type: "move", id: "story", zone: "nowhere", index: 0 }),
    WorkspaceLayoutError,
  );
  assert.strictEqual(model.snapshot(), before);
  assert.throws(
    () => model.apply({
      type: "resize-pair",
      zone: "center",
      before: "story",
      after: "room",
      delta: 1,
    }),
    WorkspaceLayoutError,
  );
  assert.strictEqual(model.snapshot(), before);
});

test("reset is canonical, character-scoped, and idempotent", () => {
  const model = restore();
  model.apply({ type: "move", id: "story", zone: "right", index: 0 });
  model.apply({ type: "hide", id: "conditions" });

  const first = model.apply({ type: "reset" });
  const second = model.apply({ type: "reset" });
  assert.strictEqual(first, second);
  assert.equal(first.character, "aster");
  assert.deepEqual(first, restore().snapshot());
  assertCanonical(first);
});

test("per-character storage helpers are normalized, versioned, and isolated", () => {
  assert.equal(normalizeLayoutCharacter("  AsTeR  "), "aster");
  assert.equal(layoutStorageKey("Aster"), "despana.workspace.v1:aster");
  assert.equal(layoutStorageKey("Briar"), "despana.workspace.v1:briar");
  assert.notEqual(layoutStorageKey("Aster"), layoutStorageKey("Briar"));
  assert.equal(layoutStorageKey("A B"), "despana.workspace.v1:a%20b");
  assert.throws(() => layoutStorageKey("   "), WorkspaceLayoutError);
});
