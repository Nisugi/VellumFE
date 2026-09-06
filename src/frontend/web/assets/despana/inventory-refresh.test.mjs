import assert from "node:assert/strict";
import test from "node:test";

import {
  DEFAULT_INVENTORY_REFRESH_TIMEOUT_MS,
  InventoryRefreshTracker,
} from "./inventory-refresh.js";


function harness() {
  let nextId = 0;
  const callbacks = new Map();
  const changes = [];
  const tracker = new InventoryRefreshTracker({
    setTimeout(callback, delay) {
      const id = ++nextId;
      callbacks.set(id, { callback, delay });
      return id;
    },
    clearTimeout(id) {
      callbacks.delete(id);
    },
    onChange(state) {
      changes.push(state);
    },
  });
  return { tracker, callbacks, changes };
}


test("an unanswered inventory refresh becomes visibly retryable after the core deadline", () => {
  const { tracker, callbacks, changes } = harness();

  tracker.begin("Calvix");
  assert.equal(tracker.state.kind, "pending");
  assert.equal(callbacks.size, 1);
  const timer = [...callbacks.values()][0];
  assert.equal(timer.delay, DEFAULT_INVENTORY_REFRESH_TIMEOUT_MS);

  timer.callback();
  assert.deepEqual(tracker.state, {
    kind: "timed-out",
    character: "Calvix",
    message: "Inventory refresh timed out. Select Refresh to try again.",
  });
  assert.equal(changes.at(-1).kind, "timed-out");
});


test("only a complete tree for the requesting character completes the refresh", () => {
  const { tracker, callbacks } = harness();
  tracker.begin("calvix");

  assert.equal(tracker.receive("Rabki", { complete: true }), false);
  assert.equal(tracker.state.kind, "pending");
  assert.equal(tracker.receive("Calvix", { complete: true }), true);
  assert.deepEqual(tracker.state, {
    kind: "ready",
    character: "calvix",
    message: "Inventory refreshed.",
  });
  assert.equal(callbacks.size, 0);
});


test("incomplete and malformed trees surface retryable failure states", () => {
  const { tracker, callbacks } = harness();

  tracker.begin("Calvix");
  assert.equal(tracker.receive("Calvix", { complete: false }), true);
  assert.deepEqual(tracker.state, {
    kind: "incomplete",
    character: "Calvix",
    message: "Inventory refresh is incomplete. Select Refresh to try again.",
  });
  assert.equal(callbacks.size, 0);

  tracker.begin("Calvix");
  assert.equal(tracker.receive("Calvix", null), true);
  assert.deepEqual(tracker.state, {
    kind: "error",
    character: "Calvix",
    message: "Inventory refresh returned invalid data. Select Refresh to try again.",
  });
  assert.equal(callbacks.size, 0);
});


test("retry and transport failure replace the prior pending timer", () => {
  const { tracker, callbacks } = harness();
  tracker.begin("Calvix");
  const firstTimer = [...callbacks.values()][0];

  tracker.begin("Calvix");
  assert.equal(callbacks.size, 1);
  firstTimer.callback();
  assert.equal(tracker.state.kind, "pending", "a cancelled timer must be harmless");

  tracker.fail("Connection lost before inventory refresh completed.");
  assert.deepEqual(tracker.state, {
    kind: "error",
    character: "Calvix",
    message: "Connection lost before inventory refresh completed.",
  });
  assert.equal(callbacks.size, 0);
});
