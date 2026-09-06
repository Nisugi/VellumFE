// Regression test for the pairing-token init-order bug:
//
//   app.js:85   creates the map controller, whose factory tail
//   app.js:1266 calls loadClassicCatalog() synchronously, which fetches
//   app.js:777  `/api/v1/maps/classic?token=${options.token?.()}` where
//   app.js:70   pairingToken() prefers localStorage over the URL hash, but
//   app.js:1557 `new DesktopSession(...)` — the code that persists a fresh
//               `#token=` from the hash into localStorage (session.js:563-564)
//               — only runs ~1470 lines later.
//
// So with a STALE token already in localStorage and a FRESH token in the URL
// hash, the very first classic-catalog fetch goes out with the stale token
// and 403s. This test boots the real app.js module under a minimal fake DOM
// and asserts which token the first /api/v1/maps/classic request carries.

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const STALE = "stale-old-token";
const FRESH = "fresh-hash-token";

function makeClassList() {
  return { add() {}, remove() {}, toggle() {}, contains: () => false };
}

const documentRef = { current: null };

function makeStubElement(id = "") {
  const el = {
    id,
    get ownerDocument() { return documentRef.current; },
    hidden: false,
    inert: false,
    value: "",
    textContent: "",
    innerHTML: "",
    disabled: false,
    scrollTop: 0,
    scrollHeight: 0,
    clientHeight: 0,
    clientWidth: 0,
    offsetWidth: 100,
    offsetHeight: 100,
    style: { setProperty() {}, removeProperty() {}, getPropertyValue: () => "" },
    dataset: {},
    options: [],
    childNodes: [],
    children: [],
    classList: makeClassList(),
    parentElement: null,
    addEventListener() {},
    removeEventListener() {},
    setAttribute() {},
    removeAttribute() {},
    getAttribute: () => null,
    appendChild(child) { return child; },
    removeChild(child) { return child; },
    replaceChildren() {},
    contains: () => false,
    focus() {},
    blur() {},
    closest: () => null,
    querySelector: () => makeStubElement(),
    querySelectorAll: () => [],
    getBoundingClientRect: () => ({ left: 0, top: 0, right: 100, bottom: 100, width: 100, height: 100 }),
    getContext: () => ({
      canvas: el,
      clearRect() {}, fillRect() {}, strokeRect() {},
      beginPath() {}, moveTo() {}, lineTo() {}, arc() {}, rect() {},
      fill() {}, stroke() {}, closePath() {}, save() {}, restore() {},
      translate() {}, scale() {}, setTransform() {}, fillText() {},
      measureText: () => ({ width: 0 }),
    }),
    scrollIntoView() {},
    scrollTo() {},
    click() {},
    remove() {},
    insertBefore(child) { return child; },
  };
  el.parentElement = { ...el, observeSafe: true };
  return el;
}

class FakeStorage {
  constructor(entries = {}) { this.entries = new Map(Object.entries(entries)); }
  getItem(key) { return this.entries.get(key) ?? null; }
  setItem(key, value) { this.entries.set(key, String(value)); }
  removeItem(key) { this.entries.delete(key); }
}

class FakeWebSocket {
  constructor(url) {
    this.url = String(url);
    this.readyState = 0;
    this.onopen = null; this.onmessage = null; this.onerror = null; this.onclose = null;
  }
  send() {}
  close() { this.readyState = 3; }
}

test("first classic map catalog fetch uses the FRESH hash token, not a stale stored one", async () => {
  const fetchedUrls = [];
  const localStorage = new FakeStorage({ "vellum-token": STALE });

  // The workspace shell demands real-looking structure: five [data-zone]
  // elements, at least one [data-module], and the named shell controls.
  const zones = ["top", "bottom", "left", "right", "center"].map((zone) => {
    const el = makeStubElement(`zone-${zone}`);
    el.dataset.zone = zone;
    el.getAttribute = (name) => (name === "data-zone" ? zone : null);
    return el;
  });
  const modules = [
    "hands", "thoughts", "active-spells", "known-spells", "injuries",
    "cooldowns", "room", "story", "familiar", "map", "compass", "combat",
    "tasks", "inventory", "conditions", "vitals",
  ].map((id) => {
    const el = makeStubElement(`module-${id}`);
    el.dataset.module = id;
    el.getAttribute = (name) => (name === "data-module" ? id : null);
    return el;
  });
  const root = makeStubElement("desktop-app");
  root.querySelectorAll = (selector) => {
    if (String(selector).includes("data-zone")) return zones;
    if (String(selector).includes("data-module")) return modules;
    return [];
  };

  const documentStub = {
    getElementById: (id) => (id === "desktop-app" ? root : makeStubElement(id)),
    querySelector: () => makeStubElement(),
    querySelectorAll: () => [],
    createElement: (tag) => makeStubElement(tag),
    createDocumentFragment: () => makeStubElement("fragment"),
    createTextNode: (text) => ({ textContent: text }),
    addEventListener() {},
    removeEventListener() {},
    body: makeStubElement("body"),
    documentElement: makeStubElement("html"),
    hidden: false,
    title: "",
  };
  documentRef.current = documentStub;

  const windowStub = {
    location: {
      protocol: "http:",
      host: "127.0.0.1:8040",
      hash: `#token=${FRESH}`,
      pathname: "/despana",
      search: "",
      href: `http://127.0.0.1:8040/despana#token=${FRESH}`,
    },
    localStorage,
    addEventListener() {},
    removeEventListener() {},
    open() {},
    matchMedia: () => ({ matches: false, addEventListener() {}, addListener() {} }),
    WebSocket: FakeWebSocket,
    requestAnimationFrame: (fn) => setTimeout(fn, 0),
    cancelAnimationFrame: (id) => clearTimeout(id),
    setTimeout, clearTimeout, setInterval, clearInterval,
    navigator: { clipboard: null },
    history: { replaceState() {} },
    innerWidth: 1280,
    innerHeight: 800,
    devicePixelRatio: 1,
  };
  documentStub.defaultView = windowStub;

  const previous = {};
  const overrides = {
    window: windowStub,
    document: documentStub,
    localStorage,
    location: windowStub.location,
    navigator: windowStub.navigator,
    history: windowStub.history,
    WebSocket: FakeWebSocket,
    requestAnimationFrame: () => 0,
    cancelAnimationFrame: () => {},
    ResizeObserver: class { observe() {} unobserve() {} disconnect() {} },
    fetch: (url) => {
      fetchedUrls.push(String(url));
      return Promise.resolve({
        ok: true,
        status: 200,
        json: async () => [],
      });
    },
    addEventListener: () => {},
    removeEventListener: () => {},
    // Keep the process able to exit: app.js starts a 1s clock interval
    // (app.js:1674) and animation frames; none of them matter here.
    setInterval: () => 0,
    clearInterval: () => {},
    setTimeout: (fn, delay, ...args) => {
      const id = setTimeout(fn, delay, ...args);
      id.unref?.();
      return id;
    },
  };
  for (const [key, value] of Object.entries(overrides)) {
    previous[key] = Object.getOwnPropertyDescriptor(globalThis, key);
    Object.defineProperty(globalThis, key, { value, configurable: true, writable: true });
  }

  try {
    // Import the real module. Use a data: URL so relative imports still work
    // against the on-disk siblings — no: data: URLs break relative imports.
    // Import app.js directly instead; the fake globals are installed first.
    await import(new URL("./app.js", import.meta.url));

    // loadClassicCatalog() runs synchronously during module evaluation; its
    // fetch has been issued by now.
    const catalogRequests = fetchedUrls.filter((url) => url.includes("/api/v1/maps/classic"));
    assert.ok(catalogRequests.length >= 1, `expected a classic catalog fetch, saw: ${JSON.stringify(fetchedUrls)}`);

    const first = new URL(catalogRequests[0], "http://127.0.0.1:8040");
    assert.equal(
      first.searchParams.get("token"),
      FRESH,
      `first classic catalog fetch must carry the fresh #token= from the URL hash, ` +
      `but it carried ${JSON.stringify(first.searchParams.get("token"))} ` +
      `(stale localStorage token = ${JSON.stringify(STALE)})`,
    );
  } finally {
    for (const [key, descriptor] of Object.entries(previous)) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  }
});
