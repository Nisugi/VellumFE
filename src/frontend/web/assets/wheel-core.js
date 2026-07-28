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

// A Back seat by the slice itself: an explicit `back: true` slice from
// config, or the synthesized seat carrying the sentinel. The machine keys
// ascend/never-fire on this — never on the display index — so an explicit
// Back works anywhere in the ring. Mirrors is_back_slice (Rust).
function isBackSlice(slice) {
  return !!(slice && (slice.back || slice.command === WHEEL_BACK));
}

// Minimum wedge width; mirrors WHEEL_MIN_SPAN_DEG (Rust). Explicit spans
// below this are clamped up and the ring scaled to close.
const MIN_SPAN_DEG = 30;

// Turn a per-slice span list (null = take an even share of the leftover)
// into concrete abutting seats [{ startDeg, spanDeg }], seat 0 CENTERED at
// `startDeg` (0 = up, clockwise). Mirrors resolve_spans (Rust): explicit
// spans floored at the minimum, remainder split evenly among the free
// slices, everything scaled to close at 360 on over/underfill. All-null +
// start 0 reproduces the old even ring.
function resolveSpans(spans, startDeg) {
  const n = spans.length;
  if (n === 0) return { seats: [] };

  const explicit = spans.map((s) => (s == null ? null : Math.max(s, MIN_SPAN_DEG)));
  const explicitSum = explicit.reduce((a, s) => a + (s || 0), 0);
  const freeCount = explicit.filter((s) => s == null).length;
  const freeEach = freeCount > 0 ? Math.max((360 - explicitSum) / freeCount, 0) : 0;

  let widths = explicit.map((s) => (s == null ? freeEach : s));
  const total = widths.reduce((a, w) => a + w, 0);
  if (total <= 1e-6) {
    widths = new Array(n).fill(360 / n);
  } else if (Math.abs(total - 360) > 1e-3) {
    const scale = 360 / total;
    widths = widths.map((w) => w * scale);
  }

  // Seat 0 centered at startDeg: its leading edge is half its width earlier.
  const seats = [];
  let edge = startDeg - widths[0] / 2;
  for (const w of widths) {
    seats.push({ startDeg: edge, spanDeg: w });
    edge += w;
  }
  return { seats };
}

// Pure angular resolution: which seat's arc the stick points at, ignoring
// magnitude. Null only when there are no seats.
function seatIndexAtAngle(x, yUp, layout) {
  if (!layout.seats.length) return null;
  const start = layout.seats[0].startDeg;
  const rel = mod360((Math.atan2(x, yUp) * 180) / Math.PI - start);
  let cum = 0;
  for (let i = 0; i < layout.seats.length; i++) {
    cum += layout.seats[i].spanDeg;
    if (rel < cum) return i;
  }
  return layout.seats.length - 1; // float slop at the wrap
}

function mod360(d) {
  return ((d % 360) + 360) % 360;
}

// The aim-convention angle (deg, 0 = up, cw) of an anchor word.
function anchorAngleDeg(anchor) {
  switch (anchor) {
    case "up": return 0;
    case "up-right": return 45;
    case "right": return 90;
    case "down-right": return 135;
    case "down": return 180;
    case "down-left": return 225;
    case "left": return 270;
    case "up-left": return 315;
    default: return 180;
  }
}

// The displayed ring for a wheel level. Top level: the real slices laid
// out by their spans and rotated by `start`. Inside a folder: Back is
// appended as the LAST seat (reals keep order 0..n-1), and the ring is
// rotated by ANGLE so Back's center lands on the anchor. backAnchor
// "none" skips Back. Mirrors WheelView::build (Rust). Returns
// { slices, realIndex, layout }.
function buildWheelView(real, inFolder, backAnchor, start) {
  const startDeg = start || 0;
  const realSpans = () => real.map((s) => (s.span == null ? null : s.span));

  // An explicit `back: true` slice replaces the synthesized seat: the
  // ring is used verbatim (the user owns order, width, and rotation), so
  // the anchor rotation doesn't apply either. Mirrors WheelView::build.
  const explicitBack = real.some((s) => s.back);
  if (!inFolder || backAnchor === "none" || explicitBack) {
    return {
      slices: real.slice(),
      realIndex: real.map((_, i) => i),
      layout: resolveSpans(realSpans(), startDeg),
    };
  }

  const slices = real.slice();
  slices.push({ label: "◂ Back", command: WHEEL_BACK });
  const realIndex = real.map((_, i) => i);
  realIndex.push(null);

  const spans = realSpans();
  spans.push(null); // Back is span-less: an even share keeps today's look
  const backIdx = slices.length - 1;
  const atZero = resolveSpans(spans, 0);
  const backCenter = atZero.seats[backIdx].startDeg + atZero.seats[backIdx].spanDeg / 2;
  const rotation = anchorAngleDeg(backAnchor) - backCenter;
  return { slices, realIndex, layout: resolveSpans(spans, rotation) };
}

// The seat under the stick honoring per-slice `inner`: resolve by angle,
// then apply that seat's floor — its own `inner` (percent) if set, else the
// global deadzone. Below the floor the seat isn't aimable (null). Gates
// aiming/commit only. Mirrors seat_at_with_inner (Rust).
function seatAtWithInner(x, yUp, view, deadzone) {
  const seat = seatIndexAtAngle(x, yUp, view.layout);
  if (seat == null) return null;
  const slice = view.slices[seat];
  const floor = slice && slice.inner != null ? slice.inner / 100 : deadzone;
  if (Math.hypot(x, yUp) < floor) return null;
  return seat;
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
  if (!slice || isBackSlice(slice) || (slice.slices || []).length) return null;
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
  // Per-slice `inner` gates aiming: a seat below its own floor reads as no
  // candidate (and thus "centered", clearing the rearm latch).
  const candidate = seatAtWithInner(x, yUp, view, timing.deadzone);
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
  const slice = view.slices[candidate];
  // Back seat — synthesized (realIndex null) or an explicit back slice:
  // auto-ascend once the nav dwell elapses, never fire.
  if (real == null || isBackSlice(slice)) {
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
  MIN_SPAN_DEG,
  isBackSlice,
  resolveSpans,
  seatIndexAtAngle,
  seatAtWithInner,
  anchorAngleDeg,
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
