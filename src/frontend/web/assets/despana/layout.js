/**
 * Pure, DOM-independent workspace layout model for the Despana desktop.
 *
 * Callers restore one model from known module ids, the shipped default, and an
 * optional saved payload. All mutations cross the single `apply(intent)` seam;
 * returned snapshots are deeply frozen and safe to hand to render adapters.
 */

export const LAYOUT_VERSION = 1;
export const LAYOUT_ZONES = Object.freeze([
  "top",
  "bottom",
  "left",
  "right",
  "center",
]);

const TRACK_ZONES = Object.freeze(["top", "bottom", "left", "right"]);
const FLOWS = new Set(["horizontal", "vertical"]);
const WEIGHT_TOTAL = 1000;
const MIN_WEIGHT = 1;
const MIN_TRACK_PIXELS = 48;
const MAX_TRACK_PIXELS = 4096;
const DEFAULT_STORAGE_PREFIX = "despana.workspace";
const BUILTIN_TRACKS = Object.freeze({
  top: 150,
  bottom: 128,
  left: 250,
  right: 340,
});

export class WorkspaceLayoutError extends Error {
  constructor(code, message) {
    super(message);
    this.name = "WorkspaceLayoutError";
    this.code = code;
  }
}

export const DEFAULT_DESPANA_LAYOUT = deepFreeze({
  version: LAYOUT_VERSION,
  character: null,
  tracks: { ...BUILTIN_TRACKS },
  zones: {
    top: {
      flow: "vertical",
      modules: [
        { id: "hands", weight: 250 },
        { id: "thoughts", weight: 750 },
      ],
    },
    bottom: {
      flow: "horizontal",
      modules: [
        { id: "conditions", weight: 450 },
        { id: "vitals", weight: 550 },
      ],
    },
    left: {
      flow: "vertical",
      modules: [
        { id: "active-spells", weight: 300 },
        { id: "known-spells", weight: 260 },
        { id: "injuries", weight: 240 },
        { id: "cooldowns", weight: 200 },
      ],
    },
    right: {
      flow: "vertical",
      modules: [
        { id: "familiar", weight: 230 },
        { id: "map", weight: 300 },
        { id: "compass", weight: 120 },
        { id: "combat", weight: 140 },
        { id: "tasks", weight: 110 },
        { id: "inventory", weight: 100 },
      ],
    },
    center: {
      flow: "vertical",
      modules: [
        { id: "room", weight: 330 },
        { id: "story", weight: 670 },
      ],
    },
  },
  hidden: [],
});

/** Normalize the identity used to isolate one character's persisted layout. */
export function normalizeLayoutCharacter(character) {
  if (typeof character !== "string") {
    throw new WorkspaceLayoutError("character", "character must be a string");
  }
  const normalized = character.normalize("NFC").trim().toLowerCase();
  if (!normalized) {
    throw new WorkspaceLayoutError("character", "character must not be empty");
  }
  return normalized;
}

/** Return the versioned localStorage key; this helper never reads or writes it. */
export function layoutStorageKey(character, prefix = DEFAULT_STORAGE_PREFIX) {
  if (typeof prefix !== "string" || !prefix.trim()) {
    throw new WorkspaceLayoutError("storage-prefix", "storage prefix must not be empty");
  }
  return `${prefix}.v${LAYOUT_VERSION}:${encodeURIComponent(normalizeLayoutCharacter(character))}`;
}

/**
 * Owns one canonical layout. Use `restore` rather than constructing directly.
 */
export class WorkspaceLayout {
  static restore({
    moduleIds,
    defaults = DEFAULT_DESPANA_LAYOUT,
    saved = null,
    character,
  } = {}) {
    const identity = normalizeLayoutCharacter(character);
    const known = normalizeKnownIds(moduleIds);
    const canonicalDefault = deepFreeze(buildDefault(defaults, known, identity));
    const restored = restoreSaved(saved, known, canonicalDefault, identity);
    return new WorkspaceLayout(known, canonicalDefault, deepFreeze(restored));
  }

  constructor(known, canonicalDefault, snapshot) {
    this._known = known;
    this._default = canonicalDefault;
    this._snapshot = snapshot;
  }

  snapshot() {
    return this._snapshot;
  }

  serialize() {
    return JSON.stringify(this._snapshot);
  }

  /** Apply one validated intent atomically and return the frozen result. */
  apply(intent) {
    if (!isRecord(intent) || typeof intent.type !== "string") {
      throw new WorkspaceLayoutError("intent", "layout intent must have a type");
    }
    if (intent.type === "reset") {
      this._snapshot = this._default;
      return this._snapshot;
    }

    const draft = mutableLayout(this._snapshot);
    switch (intent.type) {
      case "move":
        this._move(draft, intent);
        break;
      case "hide":
        this._hide(draft, intent);
        break;
      case "show":
        this._show(draft, intent);
        break;
      case "set-flow":
        this._setFlow(draft, intent);
        break;
      case "resize-track":
        this._resizeTrack(draft, intent);
        break;
      case "resize-pair":
        this._resizePair(draft, intent);
        break;
      default:
        throw new WorkspaceLayoutError(
          "intent-type",
          `unknown layout intent: ${intent.type}`,
        );
    }

    assertInvariant(draft, this._known);
    this._snapshot = deepFreeze(draft);
    return this._snapshot;
  }

  _move(draft, intent) {
    const id = this._knownId(intent.id);
    const zone = requireZone(intent.zone);
    const index = requireIndex(intent.index);
    const location = locate(draft, id);

    if (location.kind === "zone" && location.zone === zone) {
      const modules = draft.zones[zone].modules;
      const [entry] = modules.splice(location.index, 1);
      modules.splice(Math.min(index, modules.length), 0, entry);
      return;
    }

    let entry;
    if (location.kind === "zone") {
      [entry] = draft.zones[location.zone].modules.splice(location.index, 1);
      normalizeZoneWeights(draft.zones[location.zone]);
    } else {
      const [hidden] = draft.hidden.splice(location.index, 1);
      entry = { id, weight: hidden.weight };
    }

    const destination = draft.zones[zone];
    destination.modules.splice(Math.min(index, destination.modules.length), 0, entry);
    normalizeZoneWeights(destination);
  }

  _hide(draft, intent) {
    const id = this._knownId(intent.id);
    const location = locate(draft, id);
    if (location.kind === "hidden") return;

    const source = draft.zones[location.zone];
    const order = logicalZoneOrder(draft, location.zone);
    const before = source.modules[location.index - 1]?.id ||
      draft.hidden.find((entry) => entry.zone === location.zone && entry.after === id)?.id ||
      null;
    const after = source.modules[location.index + 1]?.id ||
      draft.hidden.find((entry) => entry.zone === location.zone && entry.before === id)?.id ||
      null;
    const hiddenShare = hiddenWeightForZone(draft, location.zone);
    const visibleShare = Math.max(MIN_WEIGHT, WEIGHT_TOTAL - hiddenShare);
    const [entry] = source.modules.splice(location.index, 1);
    const stableWeight = source.modules.length === 0
      ? visibleShare
      : Math.max(
          MIN_WEIGHT,
          Math.min(visibleShare - source.modules.length, Math.round(
            (entry.weight / WEIGHT_TOTAL) * visibleShare,
          )),
        );
    draft.hidden.push({
      id,
      zone: location.zone,
      index: location.index,
      weight: stableWeight,
      before,
      after,
      order,
    });
    normalizeZoneWeights(source);
  }

  _show(draft, intent) {
    const id = this._knownId(intent.id);
    const location = locate(draft, id);
    if (location.kind === "zone") return;

    const [hidden] = draft.hidden.splice(location.index, 1);
    const destination = draft.zones[hidden.zone];
    const remainingHiddenShare = hiddenWeightForZone(draft, hidden.zone);
    const visibleShare = Math.max(MIN_WEIGHT, WEIGHT_TOTAL - remainingHiddenShare);
    const desiredWeight = destination.modules.length === 0
      ? WEIGHT_TOTAL
      : Math.round((hidden.weight / visibleShare) * WEIGHT_TOTAL);
    insertReservedWeight(
      destination,
      restoreIndex(destination, hidden, draft.hidden),
      { id, weight: hidden.weight },
      desiredWeight,
    );
  }

  _setFlow(draft, intent) {
    const zone = requireZone(intent.zone);
    if (!FLOWS.has(intent.flow)) {
      throw new WorkspaceLayoutError(
        "flow",
        "zone flow must be horizontal or vertical",
      );
    }
    draft.zones[zone].flow = intent.flow;
  }

  _resizeTrack(draft, intent) {
    if (!TRACK_ZONES.includes(intent.zone)) {
      throw new WorkspaceLayoutError(
        "track-zone",
        "only top, bottom, left, and right tracks are resizable",
      );
    }
    if (typeof intent.pixels !== "number" || !Number.isFinite(intent.pixels)) {
      throw new WorkspaceLayoutError("track-size", "track size must be finite");
    }
    draft.tracks[intent.zone] = clampTrack(intent.pixels);
  }

  _resizePair(draft, intent) {
    const zone = requireZone(intent.zone);
    const before = this._knownId(intent.before);
    const after = this._knownId(intent.after);
    if (typeof intent.delta !== "number" || !Number.isFinite(intent.delta)) {
      throw new WorkspaceLayoutError("resize-delta", "resize delta must be finite");
    }

    const modules = draft.zones[zone].modules;
    const beforeIndex = modules.findIndex((entry) => entry.id === before);
    if (beforeIndex < 0 || modules[beforeIndex + 1]?.id !== after) {
      throw new WorkspaceLayoutError(
        "resize-pair",
        "resize-pair modules must be adjacent and ordered in the selected zone",
      );
    }

    const first = modules[beforeIndex];
    const second = modules[beforeIndex + 1];
    const pairTotal = first.weight + second.weight;
    const nextFirst = Math.max(
      MIN_WEIGHT,
      Math.min(pairTotal - MIN_WEIGHT, Math.round(first.weight + intent.delta)),
    );
    first.weight = nextFirst;
    second.weight = pairTotal - nextFirst;
  }

  _knownId(id) {
    if (typeof id !== "string" || !this._known.has(id)) {
      throw new WorkspaceLayoutError("module", `unknown layout module: ${String(id)}`);
    }
    return id;
  }
}

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function deepFreeze(value, seen = new WeakSet()) {
  if (value === null || typeof value !== "object" || seen.has(value)) return value;
  seen.add(value);
  for (const child of Object.values(value)) deepFreeze(child, seen);
  return Object.freeze(value);
}

function normalizeKnownIds(moduleIds) {
  if (!moduleIds || typeof moduleIds[Symbol.iterator] !== "function") {
    throw new WorkspaceLayoutError("modules", "moduleIds must be iterable");
  }
  const ids = [];
  const seen = new Set();
  for (const id of moduleIds) {
    if (typeof id !== "string" || !id || id.trim() !== id || seen.has(id)) {
      throw new WorkspaceLayoutError(
        "modules",
        "moduleIds must contain unique, non-empty, trimmed strings",
      );
    }
    seen.add(id);
    ids.push(id);
  }
  if (!ids.length) {
    throw new WorkspaceLayoutError("modules", "at least one module id is required");
  }
  if (ids.length > WEIGHT_TOTAL) {
    throw new WorkspaceLayoutError("modules", "too many modules to normalize weights");
  }
  return new Set(ids);
}

function defaultModuleIds(defaults) {
  const ids = [];
  for (const zone of LAYOUT_ZONES) {
    const modules = defaults.zones[zone].modules;
    for (const entry of modules) ids.push(entry.id);
  }
  for (const entry of defaults.hidden) ids.push(entry.id);
  return ids;
}

function requireZone(zone) {
  if (!LAYOUT_ZONES.includes(zone)) {
    throw new WorkspaceLayoutError("zone", `unknown layout zone: ${String(zone)}`);
  }
  return zone;
}

function requireIndex(index) {
  if (!Number.isSafeInteger(index) || index < 0) {
    throw new WorkspaceLayoutError("index", "module index must be a non-negative integer");
  }
  return index;
}

function positiveWeight(value, fallback = MIN_WEIGHT) {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
    return fallback;
  }
  return Math.max(MIN_WEIGHT, Math.round(value));
}

function clampTrack(value) {
  const rounded = Math.round(value);
  return Math.max(MIN_TRACK_PIXELS, Math.min(MAX_TRACK_PIXELS, rounded));
}

function normalizeZoneWeights(zone) {
  normalizeModuleWeights(zone.modules);
}

function emptyMutableLayout(character) {
  return {
    version: LAYOUT_VERSION,
    character,
    tracks: { ...BUILTIN_TRACKS },
    zones: Object.fromEntries(
      LAYOUT_ZONES.map((zone) => [
        zone,
        { flow: zone === "bottom" ? "horizontal" : "vertical", modules: [] },
      ]),
    ),
    hidden: [],
  };
}

function rawEntryId(entry) {
  if (typeof entry === "string") return entry;
  return isRecord(entry) ? entry.id : null;
}

function rawEntryWeight(entry) {
  return isRecord(entry) ? entry.weight : MIN_WEIGHT;
}

function addKnownEntries(target, rawEntries, known, seen) {
  if (!Array.isArray(rawEntries)) return;
  for (const raw of rawEntries) {
    const id = rawEntryId(raw);
    if (typeof id !== "string" || !known.has(id) || seen.has(id)) continue;
    seen.add(id);
    target.push({ id, weight: positiveWeight(rawEntryWeight(raw)) });
  }
}

function buildDefault(rawDefaults, known, character) {
  const defaults = isRecord(rawDefaults) ? rawDefaults : {};
  const result = emptyMutableLayout(character);
  const seen = new Set();

  for (const zone of TRACK_ZONES) {
    result.tracks[zone] = clampTrack(
      isRecord(defaults.tracks) && Number.isFinite(defaults.tracks[zone])
        ? defaults.tracks[zone]
        : BUILTIN_TRACKS[zone],
    );
  }
  for (const zone of LAYOUT_ZONES) {
    const rawZone = isRecord(defaults.zones?.[zone]) ? defaults.zones[zone] : {};
    result.zones[zone].flow = FLOWS.has(rawZone.flow)
      ? rawZone.flow
      : result.zones[zone].flow;
    addKnownEntries(result.zones[zone].modules, rawZone.modules, known, seen);
  }
  if (Array.isArray(defaults.hidden)) {
    for (const raw of defaults.hidden) {
      const id = rawEntryId(raw);
      if (typeof id !== "string" || !known.has(id) || seen.has(id)) continue;
      const zone = LAYOUT_ZONES.includes(raw.zone) ? raw.zone : "center";
      result.hidden.push({
        id,
        zone,
        index: Number.isSafeInteger(raw.index) && raw.index >= 0 ? raw.index : 0,
        weight: positiveWeight(rawEntryWeight(raw)),
        before: normalizeAnchor(raw.before, known, id),
        after: normalizeAnchor(raw.after, known, id),
        order: normalizeOrder(raw.order, known, id),
      });
      seen.add(id);
    }
  }
  for (const id of known) {
    if (seen.has(id)) continue;
    result.zones.center.modules.push({ id, weight: MIN_WEIGHT });
  }
  for (const zone of LAYOUT_ZONES) normalizeZoneWeights(result.zones[zone]);
  return result;
}

function parseSaved(saved) {
  if (typeof saved !== "string") return saved;
  if (!saved.trim()) return null;
  try {
    return JSON.parse(saved);
  } catch {
    return null;
  }
}

function defaultPlacements(defaults) {
  const result = new Map();
  for (const zone of LAYOUT_ZONES) {
    defaults.zones[zone].modules.forEach((entry, index) => {
      result.set(entry.id, { kind: "zone", zone, index, weight: entry.weight });
    });
  }
  defaults.hidden.forEach((entry, index) => {
    result.set(entry.id, { kind: "hidden", index, ...entry });
  });
  return result;
}

function restoreSaved(savedValue, known, defaults, character) {
  const saved = parseSaved(savedValue);
  if (
    !isRecord(saved) ||
    saved.version !== LAYOUT_VERSION ||
    typeof saved.character !== "string"
  ) {
    return mutableLayout(defaults);
  }
  let savedCharacter;
  try {
    savedCharacter = normalizeLayoutCharacter(saved.character);
  } catch {
    return mutableLayout(defaults);
  }
  if (savedCharacter !== character) return mutableLayout(defaults);

  const result = emptyMutableLayout(character);
  const placements = defaultPlacements(defaults);
  const seen = new Set();

  for (const zone of TRACK_ZONES) {
    const value = isRecord(saved.tracks) ? saved.tracks[zone] : null;
    result.tracks[zone] = Number.isFinite(value)
      ? clampTrack(value)
      : defaults.tracks[zone];
  }
  for (const zone of LAYOUT_ZONES) {
    const rawZone = isRecord(saved.zones?.[zone]) ? saved.zones[zone] : {};
    result.zones[zone].flow = FLOWS.has(rawZone.flow)
      ? rawZone.flow
      : defaults.zones[zone].flow;
    addKnownEntries(result.zones[zone].modules, rawZone.modules, known, seen);
  }

  if (Array.isArray(saved.hidden)) {
    for (const raw of saved.hidden) {
      const id = rawEntryId(raw);
      if (typeof id !== "string" || !known.has(id) || seen.has(id)) continue;
      const fallback = placements.get(id);
      const fallbackZone = fallback?.zone || "center";
      const fallbackIndex = fallback?.index || 0;
      result.hidden.push({
        id,
        zone: LAYOUT_ZONES.includes(raw.zone) ? raw.zone : fallbackZone,
        index: Number.isSafeInteger(raw.index) && raw.index >= 0
          ? raw.index
          : fallbackIndex,
        weight: positiveWeight(rawEntryWeight(raw), fallback?.weight),
        before: normalizeAnchor(raw.before, known, id),
        after: normalizeAnchor(raw.after, known, id),
        order: normalizeOrder(raw.order, known, id),
      });
      seen.add(id);
    }
  }

  // A saved v1 layout predating a newly shipped module acquires that module at
  // its default placement. Existing user ordering is otherwise left alone.
  for (const id of known) {
    if (seen.has(id)) continue;
    const fallback = placements.get(id);
    if (fallback?.kind === "hidden") {
      result.hidden.push({
        id,
        zone: fallback.zone,
        index: fallback.index,
        weight: fallback.weight,
        before: fallback.before || null,
        after: fallback.after || null,
        order: fallback.order || null,
      });
    } else {
      const zone = fallback?.zone || "center";
      const modules = result.zones[zone].modules;
      modules.splice(
        Math.min(fallback?.index ?? modules.length, modules.length),
        0,
        { id, weight: fallback?.weight || MIN_WEIGHT },
      );
    }
    seen.add(id);
  }

  for (const zone of LAYOUT_ZONES) normalizeZoneWeights(result.zones[zone]);
  assertInvariant(result, known);
  return result;
}

function mutableLayout(layout) {
  return {
    version: LAYOUT_VERSION,
    character: layout.character,
    tracks: { ...layout.tracks },
    zones: Object.fromEntries(
      LAYOUT_ZONES.map((zone) => [
        zone,
        {
          flow: layout.zones[zone].flow,
          modules: layout.zones[zone].modules.map((entry) => ({ ...entry })),
        },
      ]),
    ),
    hidden: layout.hidden.map((entry) => ({ ...entry })),
  };
}

function locate(layout, id) {
  for (const zone of LAYOUT_ZONES) {
    const index = layout.zones[zone].modules.findIndex((entry) => entry.id === id);
    if (index >= 0) return { kind: "zone", zone, index };
  }
  const index = layout.hidden.findIndex((entry) => entry.id === id);
  if (index >= 0) return { kind: "hidden", index };
  throw new WorkspaceLayoutError("invariant", `layout lost module: ${id}`);
}

function normalizeAnchor(value, known, id) {
  return typeof value === "string" && value !== id && known.has(value) ? value : null;
}

function normalizeOrder(value, known, id) {
  if (!Array.isArray(value)) return null;
  const seen = new Set();
  const result = value.filter((entry) => {
    if (typeof entry !== "string" || !known.has(entry) || seen.has(entry)) return false;
    seen.add(entry);
    return true;
  });
  return result.includes(id) ? result : null;
}

function logicalZoneOrder(layout, zone) {
  const members = new Set([
    ...layout.zones[zone].modules.map((entry) => entry.id),
    ...layout.hidden.filter((entry) => entry.zone === zone).map((entry) => entry.id),
  ]);
  const existing = layout.hidden.find(
    (entry) => entry.zone === zone && Array.isArray(entry.order),
  )?.order || [];
  const result = existing.filter((id) => members.has(id));
  for (const id of members) if (!result.includes(id)) result.push(id);
  return result;
}

function hiddenWeightForZone(layout, zone) {
  return Math.min(
    WEIGHT_TOTAL - MIN_WEIGHT,
    layout.hidden
      .filter((entry) => entry.zone === zone)
      .reduce((total, entry) => total + positiveWeight(entry.weight), 0),
  );
}

function restoreIndex(destination, hidden, hiddenEntries) {
  if (Array.isArray(hidden.order)) {
    const ownIndex = hidden.order.indexOf(hidden.id);
    for (let index = ownIndex - 1; index >= 0; index -= 1) {
      const visible = destination.modules.findIndex((entry) => entry.id === hidden.order[index]);
      if (visible >= 0) return visible + 1;
    }
    for (let index = ownIndex + 1; index < hidden.order.length; index += 1) {
      const visible = destination.modules.findIndex((entry) => entry.id === hidden.order[index]);
      if (visible >= 0) return visible;
    }
  }
  const hiddenById = new Map(hiddenEntries.map((entry) => [entry.id, entry]));
  const visibleIndex = (id) => destination.modules.findIndex((entry) => entry.id === id);
  const nearestVisible = (start, direction) => {
    const visited = new Set();
    let id = start;
    while (id && !visited.has(id)) {
      visited.add(id);
      const index = visibleIndex(id);
      if (index >= 0) return index;
      id = hiddenById.get(id)?.[direction] || null;
    }
    return -1;
  };
  const before = nearestVisible(hidden.before, "before");
  if (before >= 0) return before + 1;
  const after = nearestVisible(hidden.after, "after");
  if (after >= 0) return after;
  return Math.min(hidden.index, destination.modules.length);
}

function insertReservedWeight(zone, index, entry, requestedWeight) {
  if (zone.modules.length === 0) {
    zone.modules.push({ ...entry, weight: WEIGHT_TOTAL });
    return;
  }
  const maximum = WEIGHT_TOTAL - zone.modules.length * MIN_WEIGHT;
  const weight = Math.max(MIN_WEIGHT, Math.min(maximum, requestedWeight));
  normalizeModuleWeights(zone.modules, WEIGHT_TOTAL - weight);
  zone.modules.splice(Math.min(index, zone.modules.length), 0, { ...entry, weight });
}

function normalizeModuleWeights(entries, total = WEIGHT_TOTAL) {
  if (!entries.length) return;
  const safeTotal = Math.max(entries.length * MIN_WEIGHT, Math.round(total));
  const raw = entries.map((entry) => positiveWeight(entry.weight));
  const sum = raw.reduce((result, weight) => result + weight, 0);
  const distributable = safeTotal - entries.length * MIN_WEIGHT;
  const shares = raw.map((weight, index) => {
    const exact = (weight / sum) * distributable;
    return {
      index,
      weight: Math.floor(exact) + MIN_WEIGHT,
      fraction: exact - Math.floor(exact),
    };
  });
  let remaining = safeTotal - shares.reduce((result, share) => result + share.weight, 0);
  shares
    .slice()
    .sort((a, b) => b.fraction - a.fraction || a.index - b.index)
    .slice(0, remaining)
    .forEach((share) => {
      shares[share.index].weight += 1;
    });
  for (const share of shares) entries[share.index].weight = share.weight;
}

function assertInvariant(layout, known) {
  const seen = new Set();
  for (const zone of LAYOUT_ZONES) {
    const zoneState = layout.zones[zone];
    if (!FLOWS.has(zoneState.flow)) {
      throw new WorkspaceLayoutError("invariant", `invalid flow in ${zone}`);
    }
    let total = 0;
    for (const entry of zoneState.modules) {
      if (!known.has(entry.id) || seen.has(entry.id) || entry.weight < MIN_WEIGHT) {
        throw new WorkspaceLayoutError("invariant", `invalid module placement: ${entry.id}`);
      }
      seen.add(entry.id);
      total += entry.weight;
    }
    if (zoneState.modules.length && total !== WEIGHT_TOTAL) {
      throw new WorkspaceLayoutError("invariant", `weights in ${zone} are not normalized`);
    }
  }
  for (const entry of layout.hidden) {
    if (
      !known.has(entry.id) ||
      seen.has(entry.id) ||
      !LAYOUT_ZONES.includes(entry.zone) ||
      !Number.isSafeInteger(entry.index) ||
      entry.index < 0 ||
      entry.weight < MIN_WEIGHT ||
      (entry.before !== null && (!known.has(entry.before) || entry.before === entry.id)) ||
      (entry.after !== null && (!known.has(entry.after) || entry.after === entry.id)) ||
      (entry.order !== null && (
        !Array.isArray(entry.order) ||
        !entry.order.includes(entry.id) ||
        new Set(entry.order).size !== entry.order.length ||
        entry.order.some((id) => !known.has(id))
      ))
    ) {
      throw new WorkspaceLayoutError("invariant", `invalid hidden module: ${entry.id}`);
    }
    seen.add(entry.id);
  }
  if (seen.size !== known.size) {
    throw new WorkspaceLayoutError("invariant", "every known module must appear exactly once");
  }
}

// Useful to callers that want the exact current default registry without
// duplicating module ids. The returned array is detached and mutable.
export function defaultDespanaModuleIds() {
  return defaultModuleIds(DEFAULT_DESPANA_LAYOUT);
}
