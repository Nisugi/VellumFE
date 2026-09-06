import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const layoutSource = await readFile(new URL("./layout.js", import.meta.url), "utf8");
const layoutUrl = `data:text/javascript;base64,${Buffer.from(layoutSource).toString("base64")}`;
const persistenceSource = (await readFile(
  new URL("./workspace-persistence.js", import.meta.url),
  "utf8",
)).replace('"./layout.js"', `"${layoutUrl}"`);
const moduleUrl = `data:text/javascript;base64,${Buffer.from(persistenceSource).toString("base64")}`;
const { createDesktopWorkspaceStore } = await import(moduleUrl);

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

class SelectiveStorage extends FakeStorage {
  constructor(entries = {}, failSet = () => false) {
    super(entries);
    this.failSet = failSet;
    this.setAttempts = [];
  }

  setItem(key, value) {
    this.setAttempts.push({ key: String(key), value: String(value) });
    if (this.failSet(String(key), String(value))) throw new Error("selective storage failure");
    super.setItem(key, value);
  }
}

function response(status, body = "") {
  return {
    status,
    ok: status >= 200 && status < 300,
    async text() { return body; },
  };
}

function layout(character, marker = "") {
  return JSON.stringify({
    version: 1,
    character: character.toLowerCase(),
    tracks: {},
    zones: {},
    hidden: [],
    marker,
  });
}

function envelope(character, revision, marker = "") {
  return JSON.stringify({
    storage_version: 1,
    revision,
    layout: JSON.parse(layout(character, marker)),
  });
}

test("local workspace copies remain isolated by normalized character", async () => {
  const local = new FakeStorage();
  const store = createDesktopWorkspaceStore({ localStorage: local, fetchImpl: null });

  const aster = layout("Aster", "aster-layout");
  const briar = layout("Briar", "briar-layout");
  await store.write("Aster", aster);
  await store.write("Briar", briar);

  assert.equal(store.read("ASTER"), aster);
  assert.equal(store.read("briar"), briar);
  assert.deepEqual([...local.entries.keys()].sort(), [
    "despana.workspace.v1:aster",
    "despana.workspace.v1:briar",
  ]);
});

test("authenticated Vellum workspace is the cross-port authority", async () => {
  const calls = [];
  const shared = envelope("Aster", 42, "shared");
  const store = createDesktopWorkspaceStore({
    localStorage: new FakeStorage(),
    token: () => "test-token",
    async fetchImpl(url, options) {
      calls.push({ url, options });
      return response(200, shared);
    },
  });

  assert.equal(await store.load("Aster"), layout("Aster", "shared"));
  assert.equal(calls.length, 1);
  assert.equal(calls[0].options.method, "GET");
  assert.equal(calls[0].options.headers.Authorization, "Bearer test-token");
  assert.equal(calls[0].options.cache, "no-store");
  assert.equal(
    JSON.parse(store.read("Aster")).marker,
    "shared",
  );
});

test("missing server workspace preserves the port-local migration candidate", async () => {
  const legacy = layout("Aster", "legacy-layout");
  const local = new FakeStorage({ "despana.workspace.v1:aster": legacy });
  const store = createDesktopWorkspaceStore({
    localStorage: local,
    token: "test-token",
    fetchImpl: async () => response(404),
  });

  assert.equal(store.read("Aster"), legacy);
  assert.equal(await store.load("Aster"), null);
});

test("an unversioned server workspace remains readable", async () => {
  const legacy = layout("Aster", "legacy-server-layout");
  const store = createDesktopWorkspaceStore({
    localStorage: new FakeStorage(),
    token: "test-token",
    fetchImpl: async () => response(200, legacy),
  });

  assert.equal(await store.load("Aster"), legacy);
});

test("remote writes are serialized so an older save cannot land last", async () => {
  const requests = [];
  const releases = [];
  const store = createDesktopWorkspaceStore({
    localStorage: new FakeStorage(),
    token: "test-token",
    fetchImpl(url, options) {
      requests.push({ url, options });
      return new Promise((resolve) => releases.push(() => resolve(response(204))));
    },
  });

  const first = store.write("Aster", layout("Aster", "layout-one"));
  const second = store.write("Aster", layout("Aster", "layout-two"));
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(requests.length, 1);
  assert.equal(JSON.parse(requests[0].options.body).layout.marker, "layout-one");
  assert.equal(requests[0].options.keepalive, true);

  releases.shift()();
  await first;
  await Promise.resolve();
  assert.equal(requests.length, 2);
  assert.equal(JSON.parse(requests[1].options.body).layout.marker, "layout-two");
  releases.shift()();
  await second;
});

test("remote rejection remains visible while the local copy is retained", async () => {
  const local = new FakeStorage();
  const store = createDesktopWorkspaceStore({
    localStorage: local,
    token: "test-token",
    fetchImpl: async () => response(500),
  });

  const recoverable = layout("Aster", "recoverable-layout");
  await assert.rejects(store.write("Aster", recoverable), /workspace save failed/);
  assert.equal(store.read("Aster"), recoverable);
});

test("a session conflict remains retryable instead of being mistaken for a stale write", async () => {
  const local = new FakeStorage();
  const store = createDesktopWorkspaceStore({
    localStorage: local,
    token: "test-token",
    fetchImpl: async () => response(409, "no active character"),
  });

  const recoverable = layout("Aster", "session-ended");
  await assert.rejects(store.write("Aster", recoverable), /workspace save failed \(409\)/);
  assert.equal(store.read("Aster"), recoverable);
  const durable = JSON.parse(local.getItem("despana.workspace.v1:aster"));
  assert.equal(durable.confirmed, false);
  assert.equal(durable.layout.marker, "session-ended");
  assert.deepEqual([...local.entries.keys()], ["despana.workspace.v1:aster"]);
});

test("an unsynchronized local layout is retried before an older server copy can win", async () => {
  const local = new FakeStorage();
  const failing = createDesktopWorkspaceStore({
    localStorage: local,
    token: "test-token",
    fetchImpl: async () => response(500),
  });
  const newest = layout("Aster", "new-local-layout");
  await assert.rejects(failing.write("Aster", newest));

  const calls = [];
  const recovered = createDesktopWorkspaceStore({
    localStorage: local,
    token: "test-token",
    async fetchImpl(url, options) {
      calls.push({ url, options });
      return options.method === "PUT"
        ? response(204)
        : response(200, envelope("Aster", 1, "old-server-layout"));
    },
  });

  assert.equal(await recovered.load("Aster"), newest);
  assert.deepEqual(calls.map((call) => call.options.method), ["PUT"]);
  assert.equal(JSON.parse(local.getItem("despana.workspace.v1:aster")).confirmed, true);
});

test("failure to durably replace the sole local envelope prevents the network write", async () => {
  const key = "despana.workspace.v1:aster";
  const local = new SelectiveStorage({
    [key]: layout("Aster", "legacy-local"),
  }, (candidate) => candidate === key);
  const calls = [];
  const store = createDesktopWorkspaceStore({
    localStorage: local,
    token: "test-token",
    now: () => 100,
    async fetchImpl(url, options) {
      calls.push({ url, options });
      return response(204);
    },
  });
  await assert.rejects(
    store.write("Aster", layout("Aster", "not-durable")),
    /could not be persisted/,
  );
  assert.equal(store.read("Aster"), layout("Aster", "legacy-local"));
  assert.equal(calls.length, 0);
});

test("a newer cross-tab envelope survives an older acknowledgement and network failure", async () => {
  const key = "despana.workspace.v1:aster";
  const local = new SelectiveStorage({}, (candidate, value) => {
    if (candidate !== key) return false;
    const parsed = JSON.parse(value);
    return parsed.confirmed === true && parsed.layout.marker === "older";
  });
  const olderRequests = [];
  const older = createDesktopWorkspaceStore({
    localStorage: local,
    token: "test-token",
    now: () => 100,
    fetchImpl(url, options) {
      return new Promise((resolve) => olderRequests.push({ url, options, resolve }));
    },
  });
  const newer = createDesktopWorkspaceStore({
    localStorage: local,
    token: "test-token",
    now: () => 200,
    fetchImpl: async () => response(500),
  });

  const olderSave = older.write("Aster", layout("Aster", "older"));
  await Promise.resolve();
  await Promise.resolve();
  await assert.rejects(newer.write("Aster", layout("Aster", "newer")));
  assert.equal(newer.read("Aster"), layout("Aster", "newer"));

  olderRequests[0].resolve(response(204));
  await olderSave;
  assert.equal(older.read("Aster"), layout("Aster", "newer"));
  assert.equal(
    local.setAttempts.some(({ value }) => {
      const parsed = JSON.parse(value);
      return parsed.confirmed === true && parsed.layout.marker === "older";
    }),
    false,
  );

  const calls = [];
  const recovering = createDesktopWorkspaceStore({
    localStorage: local,
    token: "test-token",
    async fetchImpl(url, options) {
      calls.push({ url, options });
      return response(204);
    },
  });
  assert.equal(await recovering.load("Aster"), layout("Aster", "newer"));
  assert.equal(JSON.parse(calls[0].options.body).layout.marker, "newer");
  assert.equal(JSON.parse(local.getItem(key)).confirmed, true);
});

test("a stale write from an older store cannot replace a newer tab layout", async () => {
  const local = new FakeStorage();
  const requests = [];
  const fetchImpl = (url, options) => new Promise((resolve) => {
    requests.push({ url, options, resolve });
  });
  const older = createDesktopWorkspaceStore({
    localStorage: local,
    token: "test-token",
    fetchImpl,
    now: () => 100,
  });
  const newer = createDesktopWorkspaceStore({
    localStorage: local,
    token: "test-token",
    fetchImpl,
    now: () => 200,
  });

  const oldSave = older.write("Aster", layout("Aster", "old"));
  await Promise.resolve();
  await Promise.resolve();
  const newSave = newer.write("Aster", layout("Aster", "new"));
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(requests.length, 2);
  assert.ok(
    JSON.parse(requests[0].options.body).revision < JSON.parse(requests[1].options.body).revision,
  );

  requests[1].resolve(response(204));
  await newSave;
  requests[0].resolve(response(412, requests[1].options.body));
  const oldResult = await oldSave;
  assert.equal(oldResult.superseded, true);
  assert.equal(older.read("Aster"), layout("Aster", "new"));
  assert.deepEqual([...local.entries.keys()], ["despana.workspace.v1:aster"]);
});

test("small writes use keepalive and oversized writes use ordinary fetch", async () => {
  const calls = [];
  const store = createDesktopWorkspaceStore({
    localStorage: new FakeStorage(),
    token: "test-token",
    now: (() => { let revision = 300; return () => revision += 1; })(),
    async fetchImpl(url, options) {
      calls.push({ url, options });
      return response(204);
    },
  });

  await store.write("Aster", layout("Aster", "small"));
  await store.write("Aster", layout("Aster", "x".repeat(50 * 1024)));
  assert.deepEqual(calls.map((call) => call.options.keepalive), [true, false]);
});

test("flush immediately dispatches the newest queued save with unload protection", async () => {
  const requests = [];
  const store = createDesktopWorkspaceStore({
    localStorage: new FakeStorage(),
    token: "test-token",
    fetchImpl(url, options) {
      return new Promise((resolve) => requests.push({ url, options, resolve }));
    },
  });

  const first = store.write("Aster", layout("Aster", "first"));
  const newest = store.write("Aster", layout("Aster", "newest"));
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(requests.length, 1);

  const flushed = store.flush("Aster");
  assert.equal(requests.length, 2);
  assert.equal(JSON.parse(requests[1].options.body).layout.marker, "newest");
  assert.equal(requests[1].options.keepalive, true);

  requests[1].resolve(response(204));
  await flushed;
  requests[0].resolve(response(412, requests[1].options.body));
  await first;
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(requests.length, 3);
  requests[2].resolve(response(412, requests[1].options.body));
  await newest;
});

test("a stale separate-origin store adopts the winner and advances beyond its revision", async () => {
  let server = envelope("Aster", 500, "winner");
  const calls = [];
  const staleOrigin = createDesktopWorkspaceStore({
    localStorage: new FakeStorage(),
    token: "test-token",
    now: () => 100,
    async fetchImpl(url, options) {
      const incoming = JSON.parse(options.body);
      calls.push(incoming);
      const current = JSON.parse(server);
      if (incoming.revision <= current.revision) return response(412, server);
      server = options.body;
      return response(204);
    },
  });

  const conflict = await staleOrigin.write("Aster", layout("Aster", "loser"));
  assert.equal(conflict.superseded, true);
  assert.equal(conflict.layout, layout("Aster", "winner"));
  assert.equal(staleOrigin.read("Aster"), layout("Aster", "winner"));

  await staleOrigin.write("Aster", layout("Aster", "after-conflict"));
  assert.deepEqual(calls.map((call) => call.revision), [100, 501]);
  assert.equal(JSON.parse(server).layout.marker, "after-conflict");
});
