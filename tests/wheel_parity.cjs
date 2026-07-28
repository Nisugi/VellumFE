// Golden-vector parity check for the shipped JS wheel core.
//
//   node tests/wheel_parity.cjs
//
// Drives src/frontend/web/assets/wheel-core.js — the EXACT bytes the phone
// runs — through tests/data/wheel_golden.json, the same truth table the
// Rust machine is tested against (gamepad.rs wheel_tests::
// golden_vectors_match_the_rust_machine, in `cargo test`). A change on one
// side only turns the other side's run red instead of the phone firing a
// different slice than the desktop. Keep this runner semantically
// identical to the Rust one.
"use strict";

const path = require("path");
const wc = require(path.join(__dirname, "..", "src", "frontend", "web", "assets", "wheel-core.js"));
const data = require(path.join(__dirname, "data", "wheel_golden.json"));

let failures = 0;
function check(actual, expected, label) {
  const a = JSON.stringify(actual === undefined ? null : actual);
  const b = JSON.stringify(expected === undefined ? null : expected);
  if (a !== b) {
    failures += 1;
    console.error(`FAIL ${label}: got ${a}, want ${b}`);
  }
}

// ---- geometry -------------------------------------------------------------
for (const c of data.geometry) {
  check(
    wc.wheelSliceAt(c.x, c.yUp, c.count, c.deadzone),
    c.expect,
    `geometry x=${c.x} yUp=${c.yUp} count=${c.count} dz=${c.deadzone}`,
  );
}

// Back placement (back_placement_angle) and the in-folder rust_only_
// scenarios are checked ONLY on the Rust side until B7: the desktop now
// places Back by angle, while this shipped wheel-core.js still uses the
// pre-span display-index rotation and exposes no seat angles. B7 ports the
// angle scheme here and folds those groups back into the shared contract.

// ---- scenarios (shared: geometry + fire-mode state machine) ---------------
const TIMING_BASE = { deadzone: 0.5, aimMs: 150, navMs: 150, edgeThreshold: 0.9, retractDelta: 0.1 };

for (const sc of data.scenarios) {
  const folders = sc.ring.folders || [];
  const real = sc.ring.labels.map((label, i) =>
    folders.includes(i) ? { label, slices: [{ label: "child" }] } : { label },
  );
  const view = wc.buildWheelView(real, !!sc.ring.inFolder, sc.ring.anchor || "down");
  const timing = { ...TIMING_BASE, fireMode: sc.fireMode || "release" };
  const ui = {
    path: (sc.initialPath || []).slice(),
    aimed: null,
    candidate: null,
    candidateSince: null,
    rearmUntilCenter: sc.initialRearm === true,
    peakMagnitude: 0,
  };

  let fired = null;
  for (const frame of sc.frames) {
    const out = wc.wheelAimStep(ui, view, timing, frame.x, frame.y, frame.t);
    if (out.fire != null) {
      fired = out.fire;
      break; // the wheel closes on a mid-hold fire
    }
  }

  const expect = sc.expect;
  if ("fired" in expect) check(fired, expect.fired, `${sc.name}: fired`);
  if ("aimed" in expect) check(ui.aimed, expect.aimed, `${sc.name}: aimed`);
  if ("path" in expect) check(ui.path, expect.path, `${sc.name}: path`);
  if ("releaseReal" in expect) {
    const got = ui.aimed == null ? null : wc.leafRealAt(view, ui.aimed);
    check(got, expect.releaseReal, `${sc.name}: releaseReal`);
  }
}

const total = data.geometry.length + data.scenarios.length;
if (failures) {
  console.error(`wheel parity: ${failures} failure(s) across ${total} vector groups`);
  process.exit(1);
}
console.log(`wheel parity OK: ${total} vector groups match the shipped wheel-core.js`);
