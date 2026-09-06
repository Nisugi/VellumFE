import { layoutStorageKey } from "./layout.js";

const DEFAULT_ENDPOINT = "/api/v1/presentations/despana/workspace";
const STORAGE_ENVELOPE_VERSION = 1;
const KEEPALIVE_MAX_BYTES = 48 * 1024;

/**
 * Persist Despana's workspace without coupling it to a server port.
 *
 * Each character has one origin-local, revisioned envelope. It is written
 * before any network request and remains marked unconfirmed until Vellum has
 * accepted that exact revision. Vellum's authenticated file is the cross-port
 * authority and independently verifies the active character.
 */
export function createDesktopWorkspaceStore({
  localStorage = null,
  fetchImpl = globalThis.fetch?.bind(globalThis),
  token = () => "",
  endpoint = DEFAULT_ENDPOINT,
  now = () => Date.now(),
} = {}) {
  const saveChains = new Map();

  function localRead(key) {
    if (!localStorage || typeof localStorage.getItem !== "function") return null;
    try {
      const value = localStorage.getItem(key);
      return typeof value === "string" && value ? value : null;
    } catch {
      return null;
    }
  }

  function localWrite(key, value) {
    if (!localStorage || typeof localStorage.setItem !== "function") return false;
    try {
      localStorage.setItem(key, String(value));
      return true;
    } catch {
      return false;
    }
  }

  function encodeEnvelope(layout, revision, confirmed = false) {
    let parsed;
    try {
      parsed = JSON.parse(String(layout));
    } catch (error) {
      throw new Error("workspace layout is not valid JSON", { cause: error });
    }
    const remote = {
      storage_version: STORAGE_ENVELOPE_VERSION,
      revision,
      layout: parsed,
    };
    return Object.freeze({
      storage_version: STORAGE_ENVELOPE_VERSION,
      revision,
      confirmed,
      layout: String(layout),
      body: JSON.stringify(remote),
      localBody: JSON.stringify({ ...remote, confirmed }),
    });
  }

  function decodeEnvelope(raw, confirmedByDefault = false) {
    if (typeof raw !== "string" || !raw) return null;
    try {
      const parsed = JSON.parse(raw);
      if (
        parsed?.storage_version !== STORAGE_ENVELOPE_VERSION ||
        !Number.isSafeInteger(parsed.revision) ||
        parsed.revision < 0 ||
        !parsed.layout ||
        typeof parsed.layout !== "object" ||
        Array.isArray(parsed.layout)
      ) {
        return null;
      }
      return encodeEnvelope(
        JSON.stringify(parsed.layout),
        parsed.revision,
        typeof parsed.confirmed === "boolean" ? parsed.confirmed : confirmedByDefault,
      );
    } catch {
      return null;
    }
  }

  function storedRecord(character) {
    return decodeEnvelope(localRead(layoutStorageKey(character)));
  }

  function read(character) {
    const raw = localRead(layoutStorageKey(character));
    return decodeEnvelope(raw)?.layout || raw;
  }

  function nextRevision(character) {
    const current = Number(now());
    const timestamp = Number.isSafeInteger(current) && current > 0 ? current : Date.now();
    return Math.max(timestamp, (storedRecord(character)?.revision || 0) + 1);
  }

  function storeRecord(character, record) {
    return localWrite(layoutStorageKey(character), record.localBody);
  }

  function authorization() {
    const value = typeof token === "function" ? token() : token;
    return typeof value === "string" && value ? `Bearer ${value}` : null;
  }

  function utf8Length(text) {
    if (typeof TextEncoder === "function") return new TextEncoder().encode(text).byteLength;
    return text.length;
  }

  async function putRemote(record, authorizationHeader) {
    const response = await fetchImpl(endpoint, {
      method: "PUT",
      cache: "no-store",
      keepalive: utf8Length(record.body) <= KEEPALIVE_MAX_BYTES,
      headers: {
        Authorization: authorizationHeader,
        "Content-Type": "application/json",
      },
      body: record.body,
    });
    if (response.status === 412) {
      const current = decodeEnvelope(await response.text(), true);
      if (!current) throw new Error("workspace conflict response was invalid");
      return Object.freeze({ status: "superseded", current });
    }
    if (!response.ok) throw new Error(`workspace save failed (${response.status})`);
    return Object.freeze({ status: "saved" });
  }

  function reconcile(character, record, result) {
    const local = storedRecord(character);
    if (result.status === "saved") {
      if (!local || local.revision <= record.revision) {
        const confirmed = encodeEnvelope(record.layout, record.revision, true);
        storeRecord(character, confirmed);
        return Object.freeze({ saved: true, revision: record.revision, layout: record.layout });
      }
      return Object.freeze({
        saved: true,
        revision: record.revision,
        layout: local.layout,
      });
    }

    const winner = result.current;
    if (local && local.revision > winner.revision) {
      return Object.freeze({
        superseded: true,
        revision: local.revision,
        layout: local.layout,
      });
    }
    storeRecord(character, winner);
    return Object.freeze({
      superseded: true,
      revision: winner.revision,
      layout: winner.layout,
    });
  }

  async function load(character) {
    if (typeof fetchImpl !== "function") return null;
    const authorizationHeader = authorization();
    if (!authorizationHeader) return null;
    const key = layoutStorageKey(character);
    const startingRaw = localRead(key);
    const local = decodeEnvelope(startingRaw);
    if (local && !local.confirmed && local.revision > 0) {
      const result = reconcile(character, local, await putRemote(local, authorizationHeader));
      return result.layout;
    }

    const response = await fetchImpl(endpoint, {
      method: "GET",
      cache: "no-store",
      headers: { Authorization: authorizationHeader },
    });
    const currentRaw = localRead(key);
    const current = decodeEnvelope(currentRaw);
    if (response.status === 404 || response.status === 409) {
      return currentRaw !== startingRaw ? (current?.layout || currentRaw) : null;
    }
    if (!response.ok) throw new Error(`workspace load failed (${response.status})`);
    const value = await response.text();
    if (!value) return currentRaw !== startingRaw ? (current?.layout || currentRaw) : null;
    const shared = decodeEnvelope(value, true);
    if (!shared) {
      // Compatibility with an unversioned server response.
      if (currentRaw !== startingRaw && current) return current.layout;
      localWrite(key, value);
      return value;
    }
    if (current && (current.revision > shared.revision || !current.confirmed)) {
      return current.layout;
    }
    storeRecord(character, shared);
    return shared.layout;
  }

  function write(character, value) {
    const record = encodeEnvelope(String(value), nextRevision(character));
    if (!storeRecord(character, record)) {
      return Promise.reject(new Error("workspace layout could not be persisted"));
    }
    if (typeof fetchImpl !== "function") return Promise.resolve();
    const authorizationHeader = authorization();
    if (!authorizationHeader) return Promise.resolve();

    const key = layoutStorageKey(character);
    const previous = saveChains.get(key) || Promise.resolve();
    const current = previous
      .catch(() => {})
      .then(async () => reconcile(
        character,
        record,
        await putRemote(record, authorizationHeader),
      ));
    saveChains.set(key, current);
    const release = () => {
      if (saveChains.get(key) === current) saveChains.delete(key);
    };
    current.then(release, release);
    return current;
  }

  function flush(character) {
    if (typeof fetchImpl !== "function") return Promise.resolve(null);
    const authorizationHeader = authorization();
    if (!authorizationHeader) return Promise.resolve(null);
    const record = storedRecord(character);
    if (!record || record.confirmed || record.revision === 0) return Promise.resolve(null);
    // putRemote calls fetchImpl before its first suspension, handing the sole
    // durable, unconfirmed envelope to the browser's keepalive queue now.
    return putRemote(record, authorizationHeader)
      .then((result) => reconcile(character, record, result));
  }

  return Object.freeze({ read, load, write, flush });
}
