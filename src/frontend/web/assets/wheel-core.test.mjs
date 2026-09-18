// node --test src/frontend/web/assets/wheel-core.test.mjs
//
// Phone-only wheel-core rules that have no Rust mirror (the geometry and
// dwell machine are covered by tests/wheel_parity.cjs). Here: which host
// wheel a touch-wheel command leaf fires against.
import test from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const WheelCore = require("./wheel-core.js");

// The built-in ring's shape: client actions plus the "Commands" folder
// that descends into the host's default wheel.
const BUILT_IN = [
  { label: "Look", client: "cmd:look" },
  { label: "Room", client: "open:room" },
  { label: "Commands", slices: "@default" },
];

test("a Commands-folder leaf fires the host default wheel, prefix stripped", () => {
  assert.deepEqual(WheelCore.touchPickTarget(BUILT_IN, [2, 4]), { key: "", path: [4] });
  assert.deepEqual(WheelCore.touchPickTarget(BUILT_IN, [2, 1, 0]), { key: "", path: [1, 0] });
});

// The regression: a user-edited touch wheel whose slice 0 runs ";xp2" fired
// as { key: "", path: [0] } — the DEFAULT wheel's slice 0, i.e. "look".
test("an edited command slice fires under the touch key, path intact", () => {
  const edited = [{ label: "Look" }, { label: "Room", client: "open:room" }];
  assert.deepEqual(WheelCore.touchPickTarget(edited, [0]), { key: "touch", path: [0] });
});

test("a leaf inside a real touch-wheel folder keeps its full path", () => {
  const edited = [{ label: "Look" }, { label: "Combat", slices: [{ label: "Attack" }] }];
  assert.deepEqual(WheelCore.touchPickTarget(edited, [1, 0]), { key: "touch", path: [1, 0] });
});

test("the returned path is a copy, and a missing ring never throws", () => {
  const path = [0];
  const target = WheelCore.touchPickTarget([{ label: "Look" }], path);
  assert.notEqual(target.path, path);
  assert.deepEqual(WheelCore.touchPickTarget(null, [3]), { key: "touch", path: [3] });
});
