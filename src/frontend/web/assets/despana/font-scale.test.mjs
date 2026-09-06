import assert from "node:assert/strict";
import test from "node:test";

import {
  DEFAULT_FONT_SCALE,
  FONT_SCALE_STORAGE_KEY,
  readFontScale,
  normalizeFontScale,
  writeFontScale,
} from "./font-scale.js";

test("font scale is bounded and aligned to the control step", () => {
  assert.equal(normalizeFontScale(72), 75);
  assert.equal(normalizeFontScale(112), 110);
  assert.equal(normalizeFontScale("118"), 120);
  assert.equal(normalizeFontScale(240), 200);
  assert.equal(normalizeFontScale("not-a-number"), DEFAULT_FONT_SCALE);
});

test("font scale storage round-trips the normalized value", () => {
  const values = new Map();
  const storage = {
    getItem(key) { return values.get(key) ?? null; },
    setItem(key, value) { values.set(key, value); },
  };

  assert.equal(readFontScale(storage), DEFAULT_FONT_SCALE);
  assert.equal(writeFontScale(storage, 133), 135);
  assert.equal(values.get(FONT_SCALE_STORAGE_KEY), "135");
  assert.equal(readFontScale(storage), 135);
});

test("unavailable storage falls back without breaking the setting", () => {
  const storage = {
    getItem() { throw new Error("blocked"); },
    setItem() { throw new Error("blocked"); },
  };

  assert.equal(readFontScale(storage), DEFAULT_FONT_SCALE);
  assert.equal(writeFontScale(storage, 150), 150);
});
