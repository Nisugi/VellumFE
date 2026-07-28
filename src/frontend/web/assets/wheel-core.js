// VellumFE wheel core — the radial wheel's geometry and dwell/commit/fire
// state machine, state-free and app-independent. This file is the JS
// mirror of the Rust machine in frontend/gui/app/gamepad.rs (wheelSliceAt
// <-> wheel_slice_at, buildWheelView <-> WheelView::build, wheelAimStep
// <-> wheel_aim_step, leafRealAt <-> leaf_command_at); keep the two
// line-for-line parallel — the golden-vector parity tests drive both.
//
// Loaded as a classic script BEFORE the app.js module (globalThis.WheelCore)
// and require()-able from node, so tests exercise the exact shipped bytes.
"use strict";

(function () {

// Sentinel command marking the injected Back slice.
const WHEEL_BACK = "__wheel_back__";

// Which slice the stick aims at: slice 0 centered at the top, clockwise.
// Null inside the dead zone (`deadzone`, 0..1).
function wheelSliceAt(x, yUp, count, deadzone) {
  const dz = deadzone == null ? 0.5 : deadzone;
  if (!count || Math.hypot(x, yUp) < dz) return null;
  const step = 360 / count;
  const angle = (Math.atan2(x, yUp) * 180) / Math.PI;
  return Math.floor((angle + 360 + step / 2) / step) % count;
}

// Display index (0 = top, cw) whose seat is nearest a screen-anchor word,
// for `count` even seats. Places the reserved Back slice at its side.
function anchorDisplayIndex(anchor, count) {
  if (!count) return 0;
  const map = {
    up: [0, 1], down: [0, -1], left: [-1, 0], right: [1, 0],
    "up-left": [-1, 1], "up-right": [1, 1],
    "down-left": [-1, -1], "down-right": [1, -1],
  };
  const [ax, ay] = map[anchor] || [0, -1];
  return wheelSliceAt(ax, ay, count, 0) || 0;
}

function rotateRight(arr, n) {
  const len = arr.length;
  if (!len) return;
  n = ((n % len) + len) % len;
  const tail = arr.splice(len - n, n);
  arr.unshift(...tail);
}

// The displayed ring for a wheel level: the real slices plus, inside a
// folder, a synthetic Back slice at the configured anchor. Returns
// { slices, realIndex } where realIndex[d] is the real slice index for
// display index d, or null for the Back slice.
function buildWheelView(real, inFolder, backAnchor) {
  if (!inFolder) {
    return { slices: real.slice(), realIndex: real.map((_, i) => i) };
  }
  const count = real.length + 1;
  const back = { label: "◂ Back", command: WHEEL_BACK };
  const slices = real.slice();
  slices.push(back);
  const realIndex = real.map((_, i) => i);
  realIndex.push(null);
  // Back is last; rotate right so it lands at the anchor seat.
  const target = anchorDisplayIndex(backAnchor, count);
  const shift = (target + 1) % count;
  rotateRight(slices, shift);
  rotateRight(realIndex, shift);
  return { slices, realIndex };
}

// Retract fires once deflection falls delta below its peak. A small epsilon
// keeps the exact boundary from being lost to float rounding.
function retractShouldFire(magnitude, peak, delta) {
  return magnitude <= peak - delta + 1e-4;
}

// The real slice index of the fireable leaf at a DISPLAY seat, or null.
// The one guard shared by every fire path (release, edge, retract) so
// they can't drift: Back seats (realIndex null), folders, and the Back
// sentinel all return null. Mirrors leaf_command_at (Rust) — the phone
// fires by real-index path (commands resolve host-side), so this returns
// the real index instead of a command.
function leafRealAt(view, display) {
  const real = view.realIndex[display];
  if (real == null) return null;
  const slice = view.slices[display];
  if (!slice || slice.command === WHEEL_BACK || (slice.slices || []).length) return null;
  return real;
}

// Advance the dwell/commit/fire state machine one frame. Mutates only
// `ui` ({ path, aimed, candidate, candidateSince, rearmUntilCenter,
// peakMagnitude }); descend/ascend happen in place (path push/pop + the
// rearm latch); a leaf that must fire NOW (edge/retract modes) is
// returned as its display seat for the app layer to dispatch.
// Release-mode firing stays with the caller via leafRealAt(view, aimed).
//
// `timing`: { deadzone, aimMs, navMs, fireMode, edgeThreshold,
// retractDelta } — magnitudes 0..1, dwells in ms. `now` is an injected
// millisecond clock (performance.now() in the app; anything in tests).
//
// The re-arm latch: the stick is "neutral" when it has fallen back inside
// the dead zone (candidate null), and re-neutralizing is the ONLY thing
// that clears the latch — keyed on physical stick state, never a display
// index (indices are meaningless across a level change).
function wheelAimStep(ui, view, timing, x, yUp, now) {
  let render = false;
  const magnitude = Math.hypot(x, yUp);
  const candidate = wheelSliceAt(x, yUp, view.slices.length, timing.deadzone);
  const centered = candidate === null;
  const latchAfter = ui.rearmUntilCenter && !centered;
  const mayDwell = !latchAfter && !centered;
  ui.rearmUntilCenter = latchAfter;

  // Track the candidate and restart its dwell clock whenever it changes.
  // Any change also drops a prior commit — release only fires a leaf the
  // stick is *currently* dwelling, never a stale one.
  if (ui.candidate !== candidate) {
    ui.candidate = candidate;
    ui.candidateSince = now;
    ui.aimed = null;
    ui.peakMagnitude = 0;
    render = true;
  }

  // While the latch is up, no dwell may accrue — this is what stops a
  // still-deflected stick from chaining through nested folders.
  if (!mayDwell || candidate === null) return { fire: null, render };

  const dwelt = now - (ui.candidateSince == null ? now : ui.candidateSince);
  const real = view.realIndex[candidate];
  if (real == null) {
    // Back slice: auto-ascend once the nav dwell elapses.
    if (dwelt >= timing.navMs) {
      ui.path.pop();
      ui.aimed = null;
      ui.candidate = null;
      ui.candidateSince = null;
      ui.rearmUntilCenter = true;
      render = true;
    }
    return { fire: null, render };
  }

  // A real slice. Folder-ness reads the DISPLAY seat — view.slices is the
  // rotated ring; `real` is only for path.push on descend.
  const slice = view.slices[candidate];
  if ((slice.slices || []).length) {
    // Folders always descend on dwell — never fired by edge/retract.
    if (dwelt >= timing.navMs) {
      ui.path.push(real);
      ui.aimed = null;
      ui.candidate = null;
      ui.candidateSince = null;
      ui.rearmUntilCenter = true;
      ui.peakMagnitude = 0;
      render = true;
    }
    return { fire: null, render };
  }

  // Leaf, by fire mode.
  if (timing.fireMode === "edge") {
    // Fire the moment deflection crosses the threshold — no dwell. The
    // rearm latch above already blocks refiring across slices.
    if (magnitude >= timing.edgeThreshold) return { fire: candidate, render };
    return { fire: null, render };
  }
  if (timing.fireMode === "retract") {
    // Dwell to commit, track the deflection peak, then fire once it
    // drops retractDelta below that peak (a small inward flick).
    if (dwelt >= timing.aimMs) {
      if (ui.aimed !== candidate) {
        ui.aimed = candidate;
        ui.peakMagnitude = magnitude;
        render = true;
      }
      ui.peakMagnitude = Math.max(ui.peakMagnitude, magnitude);
      if (retractShouldFire(magnitude, ui.peakMagnitude, timing.retractDelta)) {
        return { fire: candidate, render };
      }
    }
    return { fire: null, render };
  }
  // release: commit; the release branch fires on button-up.
  if (dwelt >= timing.aimMs && ui.aimed !== candidate) {
    ui.aimed = candidate;
    render = true;
  }
  return { fire: null, render };
}

const WheelCore = {
  WHEEL_BACK,
  wheelSliceAt,
  anchorDisplayIndex,
  rotateRight,
  buildWheelView,
  retractShouldFire,
  leafRealAt,
  wheelAimStep,
};

if (typeof module !== "undefined" && module.exports) {
  module.exports = WheelCore; // node (parity tests require the shipped file)
} else {
  globalThis.WheelCore = WheelCore; // browser (classic script before app.js)
}

})();
