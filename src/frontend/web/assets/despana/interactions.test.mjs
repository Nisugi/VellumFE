import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = await readFile(new URL("./interactions.js", import.meta.url), "utf8");
const moduleUrl = `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
const {
  DesktopInteractionCoordinator,
  DesktopInteractionError,
} = await import(moduleUrl);

function nounLink(overrides = {}) {
  return {
    exist_id: "12345",
    noun: "backpack",
    text: "a leather backpack",
    coord: null,
    ...overrides,
  };
}

function menu(requestId, overrides = {}) {
  return {
    request_id: requestId,
    noun: "backpack",
    items: [
      { text: "Open", command: "open #12345", disabled: false },
      { text: "Examine", command: "look at #12345", disabled: false },
      { text: "Movement", command: "", disabled: true },
    ],
    ...overrides,
  };
}

function normalizedMenu(requestId, overrides = {}) {
  const reply = menu(requestId, overrides);
  delete reply.request_id;
  reply.requestId = requestId;
  return reply;
}

function harness() {
  const intents = [];
  const commands = [];
  const urls = [];
  let online = true;
  const coordinator = new DesktopInteractionCoordinator({
    dispatch(intent) {
      intents.push(intent);
      return { id: `link-${intent.requestId}`, status: "sent" };
    },
    submit(command) {
      commands.push(command);
      return { id: `command-${commands.length}`, status: "sent" };
    },
    isOnline: () => online,
    openUrl(url) {
      urls.push(url);
    },
  });
  return {
    coordinator,
    intents,
    commands,
    urls,
    setOnline(value) {
      online = value;
    },
  };
}

test("noun activations dispatch exact link-tap intents with monotonic ids", () => {
  const h = harness();
  const first = h.coordinator.activate(nounLink());
  const second = h.coordinator.activate(nounLink({ exist_id: "67890", noun: "sword" }));

  assert.deepEqual(first, {
    type: "pending-menu",
    requestId: 1,
    expectsMenu: true,
    receipt: { id: "link-1", status: "sent" },
  });
  assert.equal(second.requestId, 2);
  assert.deepEqual(h.intents, [
    { kind: "link-tap", requestId: 1, link: nounLink() },
    {
      kind: "link-tap",
      requestId: 2,
      link: nounLink({ exist_id: "67890", noun: "sword" }),
    },
  ]);
});

test("raw and normalized menu replies correlate while effects stay camelCase", () => {
  const h = harness();
  assert.deepEqual(h.coordinator.receiveMenu(menu(99)), {
    type: "ignored-menu",
    reason: "unknown",
    requestId: 99,
  });

  h.coordinator.activate(nounLink());
  h.coordinator.activate(nounLink({ exist_id: "2", noun: "sword" }));
  assert.deepEqual(h.coordinator.receiveMenu(menu(1)), {
    type: "ignored-menu",
    reason: "stale",
    requestId: 1,
  });

  const accepted = h.coordinator.receiveMenu(normalizedMenu(2));
  assert.equal(accepted.type, "menu");
  assert.equal(accepted.menu.requestId, 2);
  assert.equal("request_id" in accepted.menu, false);
  assert.ok(Object.isFrozen(accepted.menu));
  assert.ok(Object.isFrozen(accepted.menu.items));
  assert.ok(Object.isFrozen(accepted.menu.items[0]));
});

test("raw wire request_id remains backward-compatible", () => {
  const h = harness();
  const pending = h.coordinator.activate(nounLink());
  const accepted = h.coordinator.receiveMenu(menu(pending.requestId));

  assert.equal(accepted.type, "menu");
  assert.equal(accepted.menu.requestId, pending.requestId);
  assert.equal("request_id" in accepted.menu, false);
});

test("a correlated enabled pick submits the exact server command once", () => {
  const h = harness();
  const pending = h.coordinator.activate(nounLink());
  h.coordinator.receiveMenu(menu(pending.requestId));

  assert.deepEqual(h.coordinator.pick({ requestId: pending.requestId, index: 1 }), {
    type: "submitted",
    requestId: 1,
    index: 1,
    label: "Examine",
    receipt: { id: "command-1", status: "sent" },
  });
  assert.deepEqual(h.commands, ["look at #12345"]);
  assert.throws(
    () => h.coordinator.pick({ requestId: pending.requestId, index: 1 }),
    (error) => error instanceof DesktopInteractionError && error.code === "stale-pick",
  );
  assert.deepEqual(h.commands, ["look at #12345"]);
});

test("disabled, invalid, stale, and offline picks never submit", () => {
  const h = harness();
  const first = h.coordinator.activate(nounLink());
  h.coordinator.receiveMenu(menu(first.requestId));

  assert.throws(
    () => h.coordinator.pick({ requestId: first.requestId, index: 2 }),
    (error) => error.code === "disabled",
  );
  assert.throws(
    () => h.coordinator.pick({ requestId: first.requestId, index: 20 }),
    (error) => error.code === "pick",
  );
  assert.throws(
    () => h.coordinator.pick({ requestId: 500, index: 0 }),
    (error) => error.code === "stale-pick",
  );

  h.setOnline(false);
  assert.throws(
    () => h.coordinator.pick({ requestId: first.requestId, index: 0 }),
    (error) => error.code === "offline",
  );
  assert.deepEqual(h.commands, []);
  assert.deepEqual(h.coordinator.receiveMenu(menu(first.requestId)), {
    type: "ignored-menu",
    reason: "unknown",
    requestId: first.requestId,
  });
});

test("URL links stay local and direct or coordinate links expect no menu", () => {
  const h = harness();

  assert.deepEqual(
    h.coordinator.activate(nounLink({
      exist_id: "_url_",
      noun: "https://example.test/help",
      text: "Help",
    })),
    { type: "url", url: "https://example.test/help" },
  );
  assert.deepEqual(h.urls, ["https://example.test/help"]);
  assert.deepEqual(h.intents, []);

  const direct = h.coordinator.activate(nounLink({
    exist_id: "_direct_",
    noun: "stand",
    text: "stand",
  }));
  const coordinate = h.coordinator.activate(nounLink({
    exist_id: "-10966483",
    noun: "south",
    text: "south",
    coord: "2524,1864",
  }));
  assert.equal(direct.type, "dispatched");
  assert.equal(direct.expectsMenu, false);
  assert.equal(coordinate.type, "dispatched");
  assert.equal(coordinate.expectsMenu, false);
  assert.deepEqual(h.coordinator.receiveMenu(menu(direct.requestId)), {
    type: "ignored-menu",
    reason: "unknown",
    requestId: direct.requestId,
  });
});

test("close invalidates late replies but never reuses a request id", () => {
  const h = harness();
  const first = h.coordinator.activate(nounLink());
  assert.deepEqual(h.coordinator.close(), { type: "closed", requestId: 1 });
  assert.deepEqual(h.coordinator.close(), { type: "closed", requestId: null });
  assert.equal(h.coordinator.receiveMenu(menu(first.requestId)).type, "ignored-menu");

  const second = h.coordinator.activate(nounLink({ exist_id: "2", noun: "sword" }));
  assert.equal(second.requestId, 2);
});

test("a throwing submit is consumed and cannot be retried", () => {
  let calls = 0;
  const coordinator = new DesktopInteractionCoordinator({
    dispatch: () => ({ status: "sent" }),
    submit() {
      calls += 1;
      throw new Error("uncertain disconnect");
    },
    isOnline: () => true,
  });
  const pending = coordinator.activate(nounLink());
  coordinator.receiveMenu(menu(pending.requestId));

  assert.throws(
    () => coordinator.pick({ requestId: pending.requestId, index: 0 }),
    (error) => error instanceof DesktopInteractionError && error.code === "submit",
  );
  assert.throws(
    () => coordinator.pick({ requestId: pending.requestId, index: 0 }),
    (error) => error.code === "stale-pick",
  );
  assert.equal(calls, 1);
});

test("malformed URLs, menus, and internal commands are rejected", () => {
  const h = harness();
  assert.throws(
    () => h.coordinator.activate(nounLink({ exist_id: "_url_", noun: "javascript:alert(1)" })),
    (error) => error.code === "url",
  );

  const first = h.coordinator.activate(nounLink());
  assert.throws(
    () => h.coordinator.receiveMenu({ request_id: first.requestId, noun: "bag", items: null }),
    (error) => error.code === "menu",
  );

  const second = h.coordinator.activate(nounLink());
  h.coordinator.receiveMenu(menu(second.requestId, {
    items: [{ text: "Internal", command: "__submenu", disabled: false }],
  }));
  assert.throws(
    () => h.coordinator.pick({ requestId: second.requestId, index: 0 }),
    (error) => error.code === "command",
  );
  assert.deepEqual(h.commands, []);
});
