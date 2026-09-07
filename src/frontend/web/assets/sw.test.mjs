// node --test src/frontend/web/assets/sw.test.mjs
//
// Service-worker shell completeness (docs/mobile-runtime-fixes-proposal.md
// item 13): every same-origin script/style/manifest/icon referenced by the
// HTML pages the SW serves offline ("/" -> dashboard.html, "/play" ->
// index.html) must appear in the SHELL precache list, so an offline reload
// never boots a shell missing required globals. The SW file itself is
// EXECUTED (not regex-scraped) in a vm sandbox with stubbed SW globals, so
// the test sees the real CACHE/SHELL values and the real event handlers.
//
// What node CANNOT verify (needs a real browser, see item 13 regression
// checks): actual Cache Storage persistence, the installed-worker update
// handshake (waiting -> skipWaiting -> activate on navigation), and that the
// HTTP cache is bypassed. The activation *logic* (stale-cache deletion) and
// install/fetch logic are exercised here against the stubs.
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import url from "node:url";
import vm from "node:vm";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const read = (f) => fs.readFileSync(path.join(HERE, f), "utf8");

// ---- Execute sw.js with stubbed service-worker globals ---------------------

function loadSw() {
  const listeners = {};
  const cacheCalls = { addAll: [], put: [], match: [] };
  const deletedCaches = [];
  let cacheKeys = [];

  const fakeCache = {
    addAll: async (list) => cacheCalls.addAll.push(list),
    put: async (req, res) => cacheCalls.put.push([req, res]),
  };
  const caches = {
    open: async () => fakeCache,
    keys: async () => cacheKeys,
    delete: async (k) => (deletedCaches.push(k), true),
    match: async (req) => {
      cacheCalls.match.push(req);
      return { cachedFor: req.url };
    },
  };
  const self = {
    listeners,
    addEventListener: (name, fn) => (listeners[name] = fn),
    skipWaiting: async () => {},
    clients: { claim: async () => {} },
  };
  const sandbox = {
    self,
    caches,
    location: { origin: "https://phone.test" },
    URL,
    fetch: undefined, // set per-test
    Promise,
  };
  const code = read("sw.js") + "\n;({ CACHE, SHELL });";
  const exported = vm.runInNewContext(code, sandbox, { filename: "sw.js" });
  return {
    CACHE: exported.CACHE,
    SHELL: exported.SHELL,
    listeners,
    cacheCalls,
    deletedCaches,
    setCacheKeys: (keys) => (cacheKeys = keys),
    sandbox,
  };
}

// ---- Extract same-origin shell references from an HTML file ----------------

function shellRefs(htmlFile) {
  const html = read(htmlFile);
  const refs = new Set();
  for (const m of html.matchAll(/<script[^>]*\bsrc\s*=\s*"([^"]+)"/g)) refs.add(m[1]);
  for (const m of html.matchAll(/<link[^>]*\brel\s*=\s*"(?:stylesheet|manifest|icon|apple-touch-icon)"[^>]*\bhref\s*=\s*"([^"]+)"/g)) refs.add(m[1]);
  for (const m of html.matchAll(/<link[^>]*\bhref\s*=\s*"([^"]+)"[^>]*\brel\s*=\s*"(?:stylesheet|manifest|icon|apple-touch-icon)"/g)) refs.add(m[1]);
  // Only same-origin absolute paths matter for the SW shell.
  return [...refs].filter((r) => r.startsWith("/") && !r.startsWith("//"));
}

// The HTML pages the SW serves offline, keyed by route (see server.rs:
// "/" -> dashboard.html, "/play" -> index.html).
const SERVED_PAGES = { "/": "dashboard.html", "/play": "index.html" };

// ---- Dependency completeness ----------------------------------------------

test("every local script/style/manifest/icon in served shell HTML is in SHELL", () => {
  const { SHELL } = loadSw();
  for (const [route, file] of Object.entries(SERVED_PAGES)) {
    assert.ok(SHELL.includes(route), `route ${route} missing from SHELL`);
    for (const ref of shellRefs(file)) {
      assert.ok(
        SHELL.includes(ref),
        `${file} (served at ${route}) references ${ref}, which is missing from the sw.js SHELL list — offline reload would lose it`
      );
    }
  }
});

test("the regression this pins: core scripts are precached", () => {
  const { SHELL } = loadSw();
  for (const s of ["/char-core.js", "/webui-core.js", "/pairing-core.js", "/wheel-core.js", "/app.js"]) {
    assert.ok(SHELL.includes(s), `${s} missing from SHELL`);
  }
});

test("SHELL never caches private/dynamic endpoints", () => {
  const { SHELL } = loadSw();
  for (const p of SHELL) {
    assert.ok(!p.includes("?"), `SHELL entry ${p} carries a query string`);
    assert.ok(
      !/^\/(ws|health|api|sessions|token|pair|files)\b/.test(p),
      `SHELL must not cache dynamic/private endpoint ${p}`
    );
  }
});

// ---- Cache version ---------------------------------------------------------

test("cache version bumped past v4 so installed workers retire stale caches", () => {
  const { CACHE } = loadSw();
  const m = /^vellum-shell-v(\d+)$/.exec(CACHE);
  assert.ok(m, `unexpected cache name ${CACHE}`);
  assert.ok(Number(m[1]) >= 5, `cache version must be >= 5 (dependency set changed in v5), got ${CACHE}`);
});

// ---- Install: precaches the full SHELL ------------------------------------

test("install precaches exactly the SHELL list", async () => {
  const sw = loadSw();
  let waited;
  await sw.listeners.install({ waitUntil: (p) => (waited = p) });
  await waited;
  assert.equal(sw.cacheCalls.addAll.length, 1);
  assert.deepEqual(sw.cacheCalls.addAll[0], sw.SHELL);
});

// ---- Activate: stale-cache cleanup logic -----------------------------------

test("activation deletes stale shell caches (v4) and keeps the current one", async () => {
  const sw = loadSw();
  sw.setCacheKeys(["vellum-shell-v4", sw.CACHE, "unrelated-cache"]);
  let waited;
  await sw.listeners.activate({ waitUntil: (p) => (waited = p) });
  await waited;
  assert.ok(sw.deletedCaches.includes("vellum-shell-v4"), "stale v4 cache must be deleted on activation");
  assert.ok(!sw.deletedCaches.includes(sw.CACHE), "current cache must survive activation");
});

// ---- Fetch: network-first with offline fallback for every SHELL entry ------

function fetchEvent(sw, pathname, method = "GET", origin = "https://phone.test") {
  let responded;
  const event = {
    request: { method, url: origin + pathname },
    respondWith: (p) => (responded = p),
  };
  sw.listeners.fetch(event);
  return responded;
}

test("offline fetch falls back to cache for EVERY SHELL entry", async () => {
  const sw = loadSw();
  sw.sandbox.fetch = async () => {
    throw new TypeError("offline");
  };
  for (const p of sw.SHELL) {
    const res = await fetchEvent(sw, p);
    assert.ok(res, `no respondWith for SHELL entry ${p}`);
    assert.equal(res.cachedFor, "https://phone.test" + p, `cache fallback not consulted for ${p}`);
  }
});

test("network-first: successful fetch wins and refreshes the cache", async () => {
  const sw = loadSw();
  const fresh = { clone: () => ({ fromNet: true }), fromNet: true };
  sw.sandbox.fetch = async () => fresh;
  const res = await fetchEvent(sw, "/char-core.js");
  assert.equal(res, fresh);
  // put() is fire-and-forget; let the microtask run.
  await new Promise((r) => setImmediate(r));
  assert.equal(sw.cacheCalls.put.length, 1);
});

test("non-shell, non-GET, and cross-origin requests are not intercepted", () => {
  const sw = loadSw();
  assert.equal(fetchEvent(sw, "/ws"), undefined);
  assert.equal(fetchEvent(sw, "/sessions"), undefined);
  assert.equal(fetchEvent(sw, "/play", "POST"), undefined);
  assert.equal(fetchEvent(sw, "/app.js", "GET", "https://evil.test"), undefined);
});
