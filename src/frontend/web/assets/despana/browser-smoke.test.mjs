import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { access, readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { createServer as createTcpServer } from "node:net";
import { constants as fsConstants } from "node:fs";
import test from "node:test";

const DESPANA_DIR = new URL("./", import.meta.url);
const FIXTURE_URL = new URL(
  "../../../../../tests/fixtures/despana/briar-ws-v1.json",
  import.meta.url,
);
const DRIVER_TIMEOUT_MS = 15_000;
const TEST_TIMEOUT_MS = 45_000;

const CONTENT_TYPES = Object.freeze({
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
});

const TEST_PRELUDE = String.raw`<script>
(() => {
  const harness = {
    sockets: [],
    errors: [],
    rejections: [],
    opened: [],
  };

  class FakeWebSocket {
    static CONNECTING = 0;
    static OPEN = 1;
    static CLOSING = 2;
    static CLOSED = 3;

    constructor(url) {
      this.url = String(url);
      this.readyState = FakeWebSocket.CONNECTING;
      this.sent = [];
      this.onopen = null;
      this.onmessage = null;
      this.onerror = null;
      this.onclose = null;
      harness.sockets.push(this);
      queueMicrotask(() => {
        if (this.readyState !== FakeWebSocket.CONNECTING) return;
        this.readyState = FakeWebSocket.OPEN;
        this.onopen?.({ type: "open" });
      });
    }

    send(raw) {
      if (this.readyState !== FakeWebSocket.OPEN) {
        throw new Error("FakeWebSocket is not open");
      }
      this.sent.push(JSON.parse(String(raw)));
    }

    receive(message) {
      if (this.readyState !== FakeWebSocket.OPEN) {
        throw new Error("FakeWebSocket cannot receive while closed");
      }
      this.onmessage?.({ data: JSON.stringify(message) });
    }

    serverClose() {
      if (this.readyState === FakeWebSocket.CLOSED) return;
      this.readyState = FakeWebSocket.CLOSED;
      this.onclose?.({ code: 1006, reason: "offline smoke disconnect" });
    }

    close() {
      if (this.readyState === FakeWebSocket.CLOSED) return;
      this.readyState = FakeWebSocket.CLOSED;
      this.onclose?.({ code: 1000, reason: "client close" });
    }
  }

  addEventListener("error", (event) => {
    harness.errors.push(String(event.error?.stack || event.message || event.error || "error"));
  });
  addEventListener("unhandledrejection", (event) => {
    harness.rejections.push(String(event.reason?.stack || event.reason || "rejection"));
  });

  window.WebSocket = FakeWebSocket;
  window.open = (initialUrl = "", target = "", features = "") => {
    const opened = {
      url: String(initialUrl),
      target: String(target),
      features: String(features),
      closed: false,
      opener: window,
      close() { this.closed = true; },
    };
    opened.location = {
      assign(url) { opened.url = String(url); },
      replace(url) { opened.url = String(url); },
      get href() { return opened.url; },
      set href(url) { opened.url = String(url); },
    };
    harness.opened.push(opened);
    return opened;
  };
  window.__desktopTest = harness;
})();
</script>`;

function executable(name, environmentName, commonPaths) {
  const requested = process.env[environmentName];
  // Prefer concrete binaries over distro launcher scripts. Ubuntu's
  // /usr/bin/firefox wrapper answers --version but WebDriver correctly rejects
  // it as not being the browser executable.
  const candidates = requested ? [requested] : [...commonPaths, name];
  for (const candidate of candidates) {
    const result = spawnSync(candidate, ["--version"], {
      encoding: "utf8",
      env: { ...process.env, MOZ_HEADLESS: "1" },
    });
    if (!result.error && result.status === 0) return candidate;
  }
  throw new Error(
    `${name} is required for the explicit Despana browser smoke test. ` +
    `Install it or set ${environmentName} to its executable path.`,
  );
}

async function unusedPort() {
  const server = createTcpServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  await new Promise((resolve, reject) => server.close((error) => (
    error ? reject(error) : resolve()
  )));
  return address.port;
}

async function startAssetServer(sharedWorkspace) {
  const index = await readFile(new URL("index.html", DESPANA_DIR), "utf8");
  const instrumented = index.replace(
    '<script type="module" src="/despana/app.js"></script>',
    `${TEST_PRELUDE}\n  <script type="module" src="/despana/app.js"></script>`,
  );
  assert.notEqual(instrumented, index, "desktop index must expose its app module script");

  const server = createServer(async (request, response) => {
    try {
      const url = new URL(request.url || "/", "http://127.0.0.1");
      if (url.pathname === "/despana" || url.pathname === "/despana/") {
        response.writeHead(200, { "content-type": CONTENT_TYPES[".html"] });
        response.end(instrumented);
        return;
      }
      if (url.pathname === "/api/v1/presentations/despana/workspace") {
        if (request.headers.authorization !== "Bearer browser-smoke") {
          response.writeHead(403, { "content-type": "text/plain; charset=utf-8" });
          response.end("pairing token required");
          return;
        }
        if (request.method === "GET") {
          if (!sharedWorkspace.value) {
            response.writeHead(404, { "cache-control": "no-store" });
            response.end("workspace not saved");
          } else {
            response.writeHead(200, {
              "content-type": "application/json",
              "cache-control": "no-store",
            });
            response.end(sharedWorkspace.value);
          }
          return;
        }
        if (request.method === "PUT") {
          let body = "";
          for await (const chunk of request) body += String(chunk);
          sharedWorkspace.value = body;
          sharedWorkspace.writes += 1;
          response.writeHead(204, { "cache-control": "no-store" });
          response.end();
          return;
        }
      }
      const match = /^\/despana\/([a-z-]+\.(?:js|css))$/.exec(url.pathname);
      if (!match) {
        response.writeHead(404);
        response.end("not found");
        return;
      }
      const asset = new URL(match[1], DESPANA_DIR);
      await access(asset, fsConstants.R_OK);
      const extension = match[1].endsWith(".css") ? ".css" : ".js";
      response.writeHead(200, { "content-type": CONTENT_TYPES[extension] });
      response.end(await readFile(asset));
    } catch (error) {
      response.writeHead(500, { "content-type": "text/plain; charset=utf-8" });
      response.end(String(error));
    }
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  return {
    server,
    port: server.address().port,
    async close() {
      await new Promise((resolve, reject) => server.close((error) => (
        error ? reject(error) : resolve()
      )));
    },
  };
}

async function waitFor(check, description, timeoutMs = DRIVER_TIMEOUT_MS) {
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const result = await check();
      if (result) return result;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(
    `timed out waiting for ${description}${lastError ? `: ${lastError.message}` : ""}`,
  );
}

async function startWebDriver(geckodriver, firefox) {
  const port = await unusedPort();
  const logs = [];
  const child = spawn(geckodriver, ["--host", "127.0.0.1", "--port", String(port)], {
    env: { ...process.env, MOZ_HEADLESS: "1" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const capture = (chunk) => {
    logs.push(String(chunk));
    if (logs.join("").length > 64_000) logs.shift();
  };
  child.stdout.on("data", capture);
  child.stderr.on("data", capture);
  child.once("error", capture);
  const base = `http://127.0.0.1:${port}`;

  await waitFor(async () => {
    if (child.exitCode !== null) {
      throw new Error(`geckodriver exited (${child.exitCode})\n${logs.join("")}`);
    }
    try {
      const response = await fetch(`${base}/status`);
      return response.ok;
    } catch {
      return false;
    }
  }, "geckodriver readiness");

  async function request(method, path, body = undefined) {
    const response = await fetch(`${base}${path}`, {
      method,
      headers: body === undefined ? {} : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const payload = await response.json().catch(() => ({}));
    if (!response.ok || payload?.value?.error) {
      const detail = payload?.value?.message || JSON.stringify(payload);
      throw new Error(`WebDriver ${method} ${path} failed: ${detail}`);
    }
    return payload.value;
  }

  let created;
  try {
    created = await request("POST", "/session", {
      capabilities: {
        alwaysMatch: {
          browserName: "firefox",
          acceptInsecureCerts: true,
          "moz:firefoxOptions": {
            binary: firefox,
            args: ["-headless"],
          },
        },
      },
    });
  } catch (error) {
    if (child.exitCode === null) child.kill("SIGTERM");
    await new Promise((resolve) => {
      if (child.exitCode !== null) resolve();
      else child.once("exit", resolve);
    });
    throw new Error(`${error.message}\n${logs.join("")}`, { cause: error });
  }
  const sessionId = created?.sessionId;
  assert.ok(sessionId, "geckodriver did not return a W3C session id");

  return {
    logs,
    async setWindowRect(width, height) {
      return request("POST", `/session/${sessionId}/window/rect`, {
        x: 0,
        y: 0,
        width,
        height,
      });
    },
    async performActions(actions) {
      return request("POST", `/session/${sessionId}/actions`, { actions });
    },
    async navigate(url) {
      await request("POST", `/session/${sessionId}/url`, { url });
    },
    async execute(script, args = []) {
      return request("POST", `/session/${sessionId}/execute/sync`, { script, args });
    },
    async close() {
      try {
        await request("DELETE", `/session/${sessionId}`);
      } finally {
        if (child.exitCode === null) child.kill("SIGTERM");
        await new Promise((resolve) => {
          if (child.exitCode !== null) resolve();
          else child.once("exit", resolve);
        });
      }
    },
  };
}

async function waitForSocket(driver, count = 1) {
  return waitFor(
    () => driver.execute(
      "return window.__desktopTest?.sockets?.length >= arguments[0] && " +
      "window.__desktopTest.sockets[arguments[0] - 1].readyState === 1;",
      [count],
    ),
    `browser fake WebSocket ${count}`,
  );
}

async function replay(driver, fixture, socketIndex = -1) {
  await driver.execute(`
    const socket = window.__desktopTest.sockets.at(arguments[1]);
    for (const message of arguments[0]) socket.receive(message);
  `, [fixture, socketIndex]);
}

test("Despana desktop composes state, interactions, and persistent workspace in Firefox", {
  timeout: TEST_TIMEOUT_MS,
}, async (context) => {
  let first = null;
  let second = null;
  let driver = null;
  context.after(async () => {
    if (driver) await driver.close();
    await Promise.all([first?.close(), second?.close()].filter(Boolean));
  });

  const geckodriver = executable("geckodriver", "GECKODRIVER", [
    "/snap/bin/geckodriver",
    "/usr/local/bin/geckodriver",
    "/usr/bin/geckodriver",
  ]);
  const firefox = executable("firefox", "FIREFOX", [
    "/snap/firefox/current/usr/lib/firefox/firefox",
    "/usr/bin/firefox",
    "/snap/bin/firefox",
  ]);
  const fixture = JSON.parse(await readFile(FIXTURE_URL, "utf8"));
  const sharedWorkspace = { value: null, writes: 0 };
  first = await startAssetServer(sharedWorkspace);
  second = await startAssetServer(sharedWorkspace);
  driver = await startWebDriver(geckodriver, firefox);

  await driver.setWindowRect(2048, 1152);
  await driver.navigate(`http://127.0.0.1:${first.port}/despana#token=browser-smoke`);
  await waitForSocket(driver);
  assert.deepEqual(await driver.execute(
    "return window.__desktopTest.sockets[0].sent;",
  ), [{ t: "auth", d: { token: "browser-smoke" } }]);
  const idle = await driver.execute(`
    return {
      hidden: document.querySelector('#vellum-idle')?.hidden,
      title: document.querySelector('#vellum-idle-title')?.textContent,
      workspaceInert: document.querySelector('#desktop-app')?.inert,
      embeddedFrames: document.querySelectorAll('#vellum-idle iframe').length,
      playLink: document.querySelector('#vellum-idle a[href="/play"]')?.target,
    };
  `);
  assert.equal(idle.hidden, false);
  assert.equal(idle.title, "Connecting to VellumFE");
  assert.equal(idle.workspaceInert, true);
  assert.equal(idle.embeddedFrames, 0);
  assert.equal(idle.playLink, "_blank");
  await replay(driver, fixture);

  await waitFor(
    () => driver.execute("return document.querySelector('#connection-status')?.textContent === 'Connected';"),
    "rendered connected snapshot",
  );
  const composition = await driver.execute(`
    return {
      modules: document.querySelectorAll('[data-module]').length,
      controls: document.querySelectorAll('[data-module] > .module-menu-button').length,
      moduleErrors: document.querySelectorAll('[data-module-error]').length,
      applicationNames: [...document.querySelectorAll('.application-name')].map((node) => node.textContent),
      title: document.querySelector('#character-title')?.textContent,
      room: document.querySelector('#room-title')?.textContent,
      spell: document.querySelector('#spells-output')?.textContent,
      inventory: document.querySelector('#inventory-output')?.textContent,
      target: document.querySelector('.target-entry')?.textContent,
      commandParent: document.querySelector('#command-form')?.closest('[data-module]')?.dataset?.module,
      commandMovable: Boolean(document.querySelector('[data-module="command"]')),
      commandDisabled: document.querySelector('#command-input')?.disabled,
      idleHidden: document.querySelector('#vellum-idle')?.hidden,
      embeddedFrames: document.querySelectorAll('#vellum-idle iframe').length,
      playLink: document.querySelector('#vellum-idle a[href="/play"]')?.target,
      mapCanvasDisplay: getComputedStyle(document.querySelector('#map-canvas')).display,
      classicMapDisplay: getComputedStyle(document.querySelector('#map-classic-stage')).display,
      middleBottomGaps: [...document.querySelectorAll('.workspace-middle > [data-zone]')].map((zone) =>
        Math.round(document.querySelector('.workspace-middle').getBoundingClientRect().bottom
          - zone.getBoundingClientRect().bottom)),
      errors: [...window.__desktopTest.errors],
      rejections: [...window.__desktopTest.rejections],
    };
  `);
  assert.equal(composition.modules, 16);
  assert.equal(composition.controls, 16);
  assert.equal(composition.moduleErrors, 0);
  assert.deepEqual(composition.applicationNames, ["Vellum Despana", "Vellum Despana"]);
  assert.equal(composition.title, "Vellum Despana - Briar - Wizard - 90");
  assert.match(composition.room, /Duskruin Arena, Sands Approach - 23780/);
  assert.match(composition.spell, /425 · Elemental Targeting/);
  assert.match(composition.inventory, /polished oak runestaff/);
  assert.match(composition.target, /arena champion/);
  assert.equal(composition.commandParent, "story");
  assert.equal(composition.commandMovable, false);
  assert.equal(composition.commandDisabled, false);
  assert.equal(composition.idleHidden, true);
  assert.equal(composition.embeddedFrames, 0);
  assert.equal(composition.playLink, "_blank");
  assert.equal(composition.mapCanvasDisplay, 'none',
    'classic mode must not render the local map canvas underneath its image');
  assert.notEqual(composition.classicMapDisplay, 'none',
    'classic mode must render the classic map stage');
  assert.deepEqual(composition.middleBottomGaps, [0, 0, 0],
    'left, center, and right zones must consume the full middle workspace height');
  assert.deepEqual(composition.errors, []);
  assert.deepEqual(composition.rejections, []);

  const titled = await driver.execute(`
    const socket = window.__desktopTest.sockets[0];
    socket.receive({
      v: 1,
      t: 'charinfo',
      seq: 70,
      d: { profession: 'Wizard', level: '100' },
    });
    return {
      documentTitle: document.title,
      windowTitle: document.querySelector('#character-title')?.textContent,
    };
  `);
  assert.deepEqual(titled, {
    documentTitle: "Vellum Despana - Briar - Wizard - 100",
    windowTitle: "Vellum Despana - Briar - Wizard - 100",
  });

  const goalsNavigation = await driver.execute(`
    const input = document.querySelector('#command-input');
    input.value = 'GOALS';
    document.querySelector('#command-form').dispatchEvent(new Event('submit', {
      bubbles: true,
      cancelable: true,
    }));
    const socket = window.__desktopTest.sockets[0];
    const sent = socket.sent.filter((frame) => frame.t === 'cmd').at(-1);
    const reserved = window.__desktopTest.opened.at(-1);
    const beforeReply = reserved?.url;
    socket.receive({
      v: 1,
      t: 'open_url',
      seq: 71,
      d: { url: 'https://www.play.net/gs4/play/cm/loader.asp?ticket=browser-smoke' },
    });
    return {
      sent,
      openedCount: window.__desktopTest.opened.length,
      beforeReply,
      afterReply: reserved?.url,
      openerCleared: reserved?.opener === null,
    };
  `);
  assert.deepEqual(goalsNavigation, {
    sent: { t: "cmd", d: { text: "GOALS" } },
    openedCount: 1,
    beforeReply: "about:blank",
    afterReply: "https://www.play.net/gs4/play/cm/loader.asp?ticket=browser-smoke",
    openerCleared: true,
  });

  const goalsLifecycle = await driver.execute(`
    const input = document.querySelector('#command-input');
    const submit = (text) => {
      input.value = text;
      document.querySelector('#command-form').dispatchEvent(new Event('submit', {
        bubbles: true,
        cancelable: true,
      }));
    };
    const socket = window.__desktopTest.sockets[0];

    submit('GOALS');
    submit('GOALS web');
    const first = window.__desktopTest.opened.at(-2);
    const second = window.__desktopTest.opened.at(-1);
    socket.receive({
      v: 1,
      t: 'open_url',
      seq: 72,
      d: { url: 'https://www.play.net/gs4/play/cm/loader.asp?ticket=first' },
    });
    const afterFirst = [first.url, second.url];
    socket.receive({
      v: 1,
      t: 'open_url',
      seq: 73,
      d: { url: 'https://www.play.net/gs4/play/cm/loader.asp?ticket=second' },
    });
    const afterSecond = [first.url, second.url];

    submit('GOALS');
    submit('GOALS web');
    const cancelled = window.__desktopTest.opened.slice(-2);
    socket.receive({
      v: 1,
      t: 'session',
      seq: 74,
      d: { state: 'idle', character: 'Briar', game: 'GS3', session_control: true },
    });
    const closedOnSessionDisconnect = cancelled.map((target) => target.closed);
    socket.receive({
      v: 1,
      t: 'session',
      seq: 75,
      d: { state: 'connected', character: 'Briar', game: 'GS3', session_control: true },
    });
    return { afterFirst, afterSecond, closedOnSessionDisconnect };
  `);
  assert.deepEqual(goalsLifecycle, {
    afterFirst: [
      "https://www.play.net/gs4/play/cm/loader.asp?ticket=first",
      "about:blank",
    ],
    afterSecond: [
      "https://www.play.net/gs4/play/cm/loader.asp?ticket=first",
      "https://www.play.net/gs4/play/cm/loader.asp?ticket=second",
    ],
    closedOnSessionDisconnect: [true, true],
  });
  const fontScale = await driver.execute(`
    const bodyBefore = Number.parseFloat(getComputedStyle(document.body).fontSize);
    const input = document.querySelector('#font-scale');
    document.querySelector('#view-menu-button').click();
    input.value = '150';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
    return {
      bodyBefore,
      bodyAfter: Number.parseFloat(getComputedStyle(document.body).fontSize),
      output: document.querySelector('#font-scale-value').textContent,
      expanded: document.querySelector('#view-menu-button').getAttribute('aria-expanded'),
      stored: localStorage.getItem('vellum-despana-font-scale-v1'),
    };
  `);
  assert.equal(fontScale.bodyBefore, 13);
  assert.equal(fontScale.bodyAfter, 19.5);
  assert.equal(fontScale.output, '150%');
  assert.equal(fontScale.expanded, 'true');
  assert.equal(fontScale.stored, '150');

  const resetFontScale = await driver.execute(`
    document.querySelector('#font-scale-reset').click();
    return {
      body: Number.parseFloat(getComputedStyle(document.body).fontSize),
      stored: localStorage.getItem('vellum-despana-font-scale-v1'),
    };
  `);
  assert.deepEqual(resetFontScale, { body: 13, stored: '100' });

  const mapGestures = await driver.execute(`
    document.querySelector('#map-mode-local').click();
    const canvas = document.querySelector('#map-canvas');
    canvas.setPointerCapture = () => {};
    const rect = canvas.getBoundingClientRect();
    const point = { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
    const fire = (type, init = {}) => canvas.dispatchEvent(new PointerEvent(type, {
      bubbles: true,
      button: 0,
      buttons: type === 'pointerup' ? 0 : 1,
      isPrimary: true,
      pointerId: init.pointerId || 41,
      clientX: init.x ?? point.x,
      clientY: init.y ?? point.y,
      ctrlKey: init.ctrlKey || false,
    }));
    const commands = () => window.__desktopTest.sockets[0].sent.filter(
      (frame) => frame.t === 'cmd'
    );

    const beforeTap = commands().length;
    fire('pointerdown');
    fire('pointerup');
    const afterTap = commands().length;
    const tapFrame = commands().at(-1);

    fire('pointerdown', { pointerId: 42, ctrlKey: true });
    fire('pointerup', { pointerId: 42, ctrlKey: true });
    const afterControlTap = commands().length;
    const controlStatus = document.querySelector('#command-status').textContent;

    fire('pointerdown', { pointerId: 43 });
    fire('pointermove', { pointerId: 43, x: point.x + 24, y: point.y + 12 });
    fire('pointerup', { pointerId: 43, x: point.x + 24, y: point.y + 12 });
    return {
      beforeTap,
      afterTap,
      tapFrame,
      afterControlTap,
      afterDrag: commands().length,
      controlStatus,
    };
  `);
  assert.equal(mapGestures.afterTap, mapGestures.beforeTap + 1);
  assert.deepEqual(mapGestures.tapFrame, { t: 'cmd', d: { text: '.go2 23780' } });
  assert.equal(mapGestures.afterControlTap, mapGestures.afterTap,
    'Control-click reports the room id without traveling');
  assert.equal(mapGestures.afterDrag, mapGestures.afterTap,
    'dragging the generated map must not dispatch travel');
  assert.match(mapGestures.controlStatus, /Map room ID(?: copied)?: 23780/);

  const staleBrowse = await driver.execute(`
    const socket = window.__desktopTest.sockets[0];
    const locationRequest = socket.sent.findLast((frame) => frame.t === 'map_locations');
    socket.receive({
      v: 1,
      t: 'map_locations',
      seq: 80,
      d: {
        request_id: locationRequest.d.request_id,
        locations: ['Duskruin Arena', 'Darkstone Castle'],
      },
    });
    const selector = document.querySelector('#map-selector');
    selector.value = 'Darkstone Castle';
    selector.dispatchEvent(new Event('change', { bubbles: true }));
    const browseRequest = socket.sent.findLast((frame) => frame.t === 'map_view');
    document.querySelector('#map-center').click();
    socket.receive({
      v: 1,
      t: 'map_browse',
      seq: 81,
      d: {
        request_id: browseRequest.d.request_id,
        location: 'Darkstone Castle',
        scene: {
          location: 'Darkstone Castle',
          sheet: 'outdoor',
          rooms: [{ i: 3717, x: 0, y: 0, e: false }],
          edges: [],
          labels: [],
        },
        error: null,
      },
    });
    const canvas = document.querySelector('#map-canvas');
    const rect = canvas.getBoundingClientRect();
    const point = { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
    const fire = (type) => canvas.dispatchEvent(new PointerEvent(type, {
      bubbles: true,
      button: 0,
      buttons: type === 'pointerup' ? 0 : 1,
      isPrimary: true,
      pointerId: 44,
      clientX: point.x,
      clientY: point.y,
    }));
    fire('pointerdown');
    fire('pointerup');
    return socket.sent.filter((frame) => frame.t === 'cmd').at(-1);
  `);
  assert.deepEqual(staleBrowse, { t: 'cmd', d: { text: '.go2 23780' } },
    'a late browse reply must not replace the live map after Center');

  const narrowVitals = await driver.execute(`
    document.querySelector('[data-module-menu="vitals"]').click();
    document.querySelector(
      '#module-menu [data-layout-action="move-zone"][data-zone="right"]'
    ).click();
    return [...document.querySelectorAll('[data-module="vitals"] .vital-status')].map((node) => ({
      text: node.textContent,
      clientWidth: node.clientWidth,
      scrollWidth: node.scrollWidth,
    }));
  `);
  assert.equal(narrowVitals.length, 3);
  for (const gauge of narrowVitals) {
    assert.ok(
      gauge.scrollWidth <= gauge.clientWidth,
      `narrow Vitals status must remain readable: ${JSON.stringify(gauge)}`,
    );
  }

  const leftResize = await driver.execute(`
    const separator = document.querySelector('[data-track-zone="left"]');
    const zone = document.querySelector('[data-zone="left"]');
    const rect = separator.getBoundingClientRect();
    return {
      x: Math.floor(rect.left) - 3,
      y: Math.round(rect.top + rect.height / 2),
      width: zone.getBoundingClientRect().width,
    };
  `);
  await driver.performActions([{
    type: "pointer",
    id: "layout-resize-mouse",
    parameters: { pointerType: "mouse" },
    actions: [
      { type: "pointerMove", duration: 0, origin: "viewport", x: leftResize.x, y: leftResize.y },
      { type: "pointerDown", button: 0 },
      { type: "pointerMove", duration: 250, origin: "viewport", x: leftResize.x + 80, y: leftResize.y },
      { type: "pointerUp", button: 0 },
    ],
  }]);
  const resizedLeftWidth = await driver.execute(
    "return document.querySelector('[data-zone=\"left\"]')?.getBoundingClientRect().width;",
  );
  assert.ok(
    resizedLeftWidth >= leftResize.width + 60,
    `pointer resize must grow the left zone (before=${leftResize.width}, after=${resizedLeftWidth})`,
  );

  const pairResize = await driver.execute(`
    const separator = document.querySelector(
      '[data-zone="center"] [data-before="room"][data-after="story"]'
    );
    const room = document.querySelector('[data-module="room"]');
    const story = document.querySelector('[data-module="story"]');
    const separatorRect = separator.getBoundingClientRect();
    return {
      x: Math.round(separatorRect.left + separatorRect.width / 2),
      y: Math.floor(separatorRect.top) - 3,
      roomHeight: room.getBoundingClientRect().height,
      storyHeight: story.getBoundingClientRect().height,
    };
  `);
  await driver.performActions([{
    type: "pointer",
    id: "module-resize-mouse",
    parameters: { pointerType: "mouse" },
    actions: [
      { type: "pointerMove", duration: 0, origin: "viewport", x: pairResize.x, y: pairResize.y },
      { type: "pointerDown", button: 0 },
      { type: "pointerMove", duration: 250, origin: "viewport", x: pairResize.x, y: pairResize.y + 80 },
      { type: "pointerUp", button: 0 },
    ],
  }]);
  const resizedPair = await driver.execute(`
    return {
      roomHeight: document.querySelector('[data-module="room"]').getBoundingClientRect().height,
      storyHeight: document.querySelector('[data-module="story"]').getBoundingClientRect().height,
    };
  `);
  assert.ok(
    resizedPair.roomHeight >= pairResize.roomHeight + 60,
    `pointer resize must grow Room (before=${pairResize.roomHeight}, after=${resizedPair.roomHeight})`,
  );
  assert.ok(
    resizedPair.storyHeight <= pairResize.storyHeight - 60,
    `pointer resize must shrink Story (before=${pairResize.storyHeight}, after=${resizedPair.storyHeight})`,
  );

  const drag = await driver.execute(`
    const source = document.querySelector('[data-module="cooldowns"] > .pane-header');
    const target = document.querySelector('[data-zone="right"] [data-module="tasks"]');
    const from = source.getBoundingClientRect();
    const to = target.getBoundingClientRect();
    return {
      fromX: Math.round(from.left + Math.min(40, from.width / 2)),
      fromY: Math.round(from.top + from.height / 2),
      toX: Math.round(to.left + to.width / 2),
      toY: Math.round(to.top + to.height / 2),
    };
  `);
  await driver.performActions([{
    type: "pointer",
    id: "layout-drag-mouse",
    parameters: { pointerType: "mouse" },
    actions: [
      { type: "pointerMove", duration: 0, origin: "viewport", x: drag.fromX, y: drag.fromY },
      { type: "pointerDown", button: 0 },
      { type: "pause", duration: 100 },
      { type: "pointerMove", duration: 350, origin: "viewport", x: drag.toX, y: drag.toY },
      { type: "pause", duration: 100 },
      { type: "pointerUp", button: 0 },
    ],
  }]);
  const dragResult = await driver.execute(`
    return {
      zone: document.querySelector('[data-module="cooldowns"]')?.parentElement?.dataset?.zone,
      status: document.querySelector('#workspace-status')?.textContent,
      persisted: localStorage.getItem('despana.workspace.v1:briar'),
    };
  `);
  assert.equal(
    dragResult.zone,
    "right",
    `pointer drag must move Cooldowns into the right zone: ${JSON.stringify(dragResult)}`,
  );

  const compass = await driver.execute(`
    const south = document.querySelector('[data-direction="south"]');
    const down = document.querySelector('[data-direction="down"]');
    const north = document.querySelector('[data-direction="north"]');
    south.click();
    return {
      southDisabled: south.disabled,
      downDisabled: down.disabled,
      northDisabled: north.disabled,
      sent: window.__desktopTest.sockets[0].sent.at(-1),
    };
  `);
  assert.deepEqual(compass, {
    southDisabled: false,
    downDisabled: false,
    northDisabled: true,
    sent: { t: "cmd", d: { text: "s" } },
  });

  const targetTap = await driver.execute(`
    document.querySelector('.target-entry').click();
    return window.__desktopTest.sockets[0].sent.at(-1);
  `);
  assert.equal(targetTap.t, "link_tap");
  assert.equal(targetTap.d.exist_id, "#fixture-briar-target-01");
  assert.equal(targetTap.d.noun, "champion");
  assert.equal(targetTap.d.request_id, 1);

  await driver.execute(`
    window.__desktopTest.sockets[0].receive({
      v: 1,
      seq: 3,
      t: "menu",
      d: {
        request_id: arguments[0],
        noun: "champion",
        items: [
          { text: "Examine", command: "look at #fixture-briar-target-01", disabled: false },
          { text: "Combat", command: "", disabled: true },
        ],
      },
    });
  `, [targetTap.d.request_id]);
  const menuDispatch = await driver.execute(`
    const menu = document.querySelector('.game-context-menu');
    const button = [...menu.querySelectorAll('button')]
      .find((candidate) => candidate.textContent === 'Examine');
    const visible = !menu.hidden;
    button.click();
    return {
      visible,
      sent: window.__desktopTest.sockets[0].sent.at(-1),
      matchingCommands: window.__desktopTest.sockets[0].sent.filter(
        (frame) => frame.t === 'cmd' && frame.d.text === 'look at #fixture-briar-target-01'
      ).length,
    };
  `);
  assert.equal(menuDispatch.visible, true);
  assert.deepEqual(menuDispatch.sent, {
    t: "cmd",
    d: { text: "look at #fixture-briar-target-01" },
  });
  assert.equal(menuDispatch.matchingCommands, 1);

  const persisted = await driver.execute(`
    document.querySelector('[data-module-menu="familiar"]').click();
    document.querySelector(
      '#module-menu [data-layout-action="move-zone"][data-zone="left"]'
    ).click();
    const key = 'despana.workspace.v1:briar';
    return {
      parentZone: document.querySelector('[data-module="familiar"]').parentElement.dataset.zone,
      layout: localStorage.getItem(key),
      cookie: document.cookie,
    };
  `);
  assert.equal(persisted.parentZone, "left");
  assert.ok(persisted.layout, "workspace mutation must persist to localStorage");
  assert.ok(
    JSON.parse(persisted.layout).layout.zones.left.modules.some(
      (entry) => entry.id === "familiar",
    ),
    "persisted layout must place Familiar in the left zone",
  );
  assert.equal(persisted.cookie, "", "workspace persistence must not write cookies");
  await waitFor(() => sharedWorkspace.writes > 0, "Vellum-owned workspace save");
  assert.equal(JSON.parse(sharedWorkspace.value).layout.character, "briar");

  const transportReservation = await driver.execute(`
    const input = document.querySelector('#command-input');
    input.value = 'GOALS';
    document.querySelector('#command-form').dispatchEvent(new Event('submit', {
      bubbles: true,
      cancelable: true,
    }));
    return window.__desktopTest.opened.length - 1;
  `);
  await driver.execute("window.__desktopTest.sockets[0].serverClose();");
  assert.equal(await driver.execute(
    "return window.__desktopTest.opened[arguments[0]].closed;",
    [transportReservation],
  ), true, "transport disconnect must close pending GOALS reservations");
  assert.equal(await driver.execute(
    "return document.querySelector('#command-status').textContent;",
  ), "The last command or action may not have reached the game and was not replayed.");
  await waitForSocket(driver, 2);
  await replay(driver, fixture, 1);
  const reconnect = await driver.execute(`
    return {
      sent: window.__desktopTest.sockets[1].sent,
      status: document.querySelector('#command-status').textContent,
    };
  `);
  assert.deepEqual(
    reconnect.sent.slice(0, 3).map((frame) => frame.t),
    ["auth", "subscribe", "resume"],
  );
  assert.equal(
    reconnect.sent.filter((frame) => frame.t === "map_locations").length,
    1,
    "Local mode must re-request its selector catalog after reconnect",
  );
  assert.equal(reconnect.sent.some((frame) => frame.t === "cmd" || frame.t === "link_tap"), false);
  assert.equal(
    reconnect.status,
    "The last command or action may not have reached the game and was not replayed.",
  );

  await driver.navigate(`http://127.0.0.1:${second.port}/despana#token=browser-smoke`);
  await waitForSocket(driver);
  await replay(driver, fixture);
  await waitFor(
    () => driver.execute(
      "return document.querySelector('[data-module=\"familiar\"]')?.parentElement?.dataset?.zone === 'left';",
    ),
    "cross-port workspace restoration",
  );
  const restored = await driver.execute(`
    return {
      parentZone: document.querySelector('[data-module="familiar"]').parentElement.dataset.zone,
      status: document.querySelector('#workspace-status').textContent,
      mirrored: Boolean(localStorage.getItem('despana.workspace.v1:briar')),
      errors: [...window.__desktopTest.errors],
      rejections: [...window.__desktopTest.rejections],
    };
  `);
  assert.equal(restored.parentZone, "left");
  assert.equal(restored.status, "Briar workspace restored");
  assert.equal(restored.mirrored, true);
  assert.deepEqual(restored.errors, []);
  assert.deepEqual(restored.rejections, []);

  const pagehideCleanup = await driver.execute(`
    const input = document.querySelector('#command-input');
    const submit = (text) => {
      input.value = text;
      document.querySelector('#command-form').dispatchEvent(new Event('submit', {
        bubbles: true,
        cancelable: true,
      }));
    };
    submit('GOALS');
    submit('GOALS web');
    const pending = window.__desktopTest.opened.slice(-2);
    window.dispatchEvent(new PageTransitionEvent('pagehide', { persisted: true }));
    return pending.map((target) => target.closed);
  `);
  assert.deepEqual(pagehideCleanup, [true, true]);
});
