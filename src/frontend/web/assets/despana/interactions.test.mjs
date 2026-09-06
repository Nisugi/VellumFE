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

function harness({ blockedReservations = [] } = {}) {
  const intents = [];
  const commands = [];
  const urls = [];
  const reservedUrls = [];
  const blocked = new Set(blockedReservations);
  let reservationAttempt = 0;
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
    reserveUrl() {
      if (blocked.has(reservationAttempt++)) return null;
      const target = {
        navigated: [],
        closed: false,
        navigate(url) {
          this.navigated.push(url);
        },
        close() {
          this.closed = true;
        },
      };
      reservedUrls.push(target);
      return target;
    },
  });
  return {
    coordinator,
    intents,
    commands,
    urls,
    reservedUrls,
    setOnline(value) {
      online = value;
    },
  };
}

test("overlapping GOALS replies navigate reserved tabs in submission order", () => {
  const h = harness();

  assert.deepEqual(h.coordinator.submit(" GOALS "), {
    id: "command-1",
    status: "sent",
  });
  assert.deepEqual(h.commands, [" GOALS "]);
  assert.equal(h.reservedUrls.length, 1);
  assert.deepEqual(h.reservedUrls[0].navigated, []);

  h.coordinator.submit("goals web");
  assert.equal(h.reservedUrls.length, 2, "browser goals web must reserve its own tab");
  assert.equal(h.reservedUrls[0].closed, false);

  assert.deepEqual(
    h.coordinator.receiveOpenUrl("https://www.play.net/gs4/play/cm/loader.asp?ticket=first"),
    {
      type: "url",
      url: "https://www.play.net/gs4/play/cm/loader.asp?ticket=first",
      reserved: true,
    },
  );
  assert.deepEqual(h.reservedUrls[0].navigated, [
    "https://www.play.net/gs4/play/cm/loader.asp?ticket=first",
  ]);
  assert.deepEqual(h.reservedUrls[1].navigated, []);

  h.coordinator.receiveOpenUrl("https://www.play.net/gs4/play/cm/loader.asp?ticket=second");
  assert.deepEqual(h.reservedUrls[1].navigated, [
    "https://www.play.net/gs4/play/cm/loader.asp?ticket=second",
  ]);
});

test("a blocked reservation leaves a FIFO tombstone for its reply", () => {
  const h = harness({ blockedReservations: [0] });

  h.coordinator.submit("goals");
  h.coordinator.submit("goals web");
  assert.equal(h.reservedUrls.length, 1, "only the second popup reservation succeeds");

  const first = "https://www.play.net/gs4/play/cm/loader.asp?ticket=blocked";
  assert.deepEqual(h.coordinator.receiveOpenUrl(first), {
    type: "url",
    url: first,
    reserved: false,
    dropped: true,
  });
  assert.deepEqual(h.urls, [], "a tombstoned reply must not use the popup fallback");
  assert.deepEqual(h.reservedUrls[0].navigated, []);

  const second = "https://www.play.net/gs4/play/cm/loader.asp?ticket=reserved";
  assert.deepEqual(h.coordinator.receiveOpenUrl(second), {
    type: "url",
    url: second,
    reserved: true,
  });
  assert.deepEqual(h.reservedUrls[0].navigated, [second]);
});

test("documented dotted GOALS web command reserves its browser tab", () => {
  const h = harness();

  h.coordinator.submit(" .GOALS web ");

  assert.deepEqual(h.commands, [" .GOALS web "]);
  assert.equal(h.reservedUrls.length, 1);
});

test("cancelling pending GOALS closes and discards every reservation", () => {
  const h = harness();
  h.coordinator.submit("goals");
  h.coordinator.submit("goals web");

  h.coordinator.cancelPendingUrls();
  assert.deepEqual(h.reservedUrls.map((target) => target.closed), [true, true]);

  const late = "https://www.play.net/gs4/play/cm/loader.asp?ticket=late";
  assert.deepEqual(h.coordinator.receiveOpenUrl(late), {
    type: "url",
    url: late,
    reserved: false,
    dropped: true,
  });
  assert.deepEqual(h.urls, [], "a late addressed reply must not open a new popup");
  assert.deepEqual(h.reservedUrls.map((target) => target.navigated), [[], []]);
});

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
