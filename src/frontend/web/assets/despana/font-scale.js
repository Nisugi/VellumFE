export const FONT_SCALE_STORAGE_KEY = "vellum-despana-font-scale-v1";
export const DEFAULT_FONT_SCALE = 100;
export const MIN_FONT_SCALE = 75;
export const MAX_FONT_SCALE = 200;
export const FONT_SCALE_STEP = 5;

export function normalizeFontScale(value) {
  const numeric = typeof value === "number"
    ? value
    : Number.parseFloat(String(value ?? ""));
  if (!Number.isFinite(numeric)) return DEFAULT_FONT_SCALE;
  const snapped = Math.round(numeric / FONT_SCALE_STEP) * FONT_SCALE_STEP;
  return Math.max(MIN_FONT_SCALE, Math.min(MAX_FONT_SCALE, snapped));
}

export function readFontScale(storage) {
  try {
    const stored = storage?.getItem?.(FONT_SCALE_STORAGE_KEY);
    return stored === null || stored === undefined
      ? DEFAULT_FONT_SCALE
      : normalizeFontScale(stored);
  } catch {
    return DEFAULT_FONT_SCALE;
  }
}

export function writeFontScale(storage, value) {
  const normalized = normalizeFontScale(value);
  try {
    storage?.setItem?.(FONT_SCALE_STORAGE_KEY, String(normalized));
  } catch {
    // The visual setting still applies when storage is unavailable.
  }
  return normalized;
}
