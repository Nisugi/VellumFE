// VellumFE character-switch core — parsing and identity rules for the
// app-shell character wheel, state-free and app-independent.
//
// The native shell appends two parallel fragment params to the boot URL:
//   chars=name@host:port,…     (name and host percent-encoded)
//   charids=id,…               (percent-encoded stable store IDs, same order)
// Names are display labels only; the ID is the identity a wheel pick sends
// back through vellum://remote/connect?id=… (tokens never leave native
// storage). An older shell omits charids= — entries then fall back to
// name-keyed identity and the legacy name-based connect URL, which the
// shells resolve only when exactly one saved entry matches.
//
// Loaded as a classic script BEFORE the app.js module (globalThis.CharCore)
// and require()-able from node, so tests exercise the exact shipped bytes.
"use strict";

(function () {

// Parse the shell's character fragment out of a location.hash string.
// Returns [{ id, name, hostPort }] — id is null when the shell sent no
// usable charids list. IDs are zipped by position; a charids list whose
// length differs from the (pre-filter) chars list is discarded wholesale
// rather than risk misaligned identities.
function parseShellChars(hash) {
  const m = (hash || "").match(/(?:^#|&)chars=([^&]+)/);
  if (!m) return [];
  const rawChars = m[1].split(",");

  const idm = (hash || "").match(/(?:^#|&)charids=([^&]+)/);
  let ids = null;
  if (idm) {
    const rawIds = idm[1].split(",");
    if (rawIds.length === rawChars.length) {
      ids = rawIds.map((s) => {
        try {
          const d = decodeURIComponent(s);
          return d || null;
        } catch {
          return null;
        }
      });
    }
  }

  const out = [];
  for (let i = 0; i < rawChars.length; i++) {
    const entry = rawChars[i];
    const at = entry.indexOf("@");
    const colon = entry.lastIndexOf(":");
    if (at <= 0 || colon <= at) continue;
    const port = Number(entry.slice(colon + 1));
    let host;
    let name;
    try {
      name = decodeURIComponent(entry.slice(0, at));
      host = decodeURIComponent(entry.slice(at + 1, colon));
    } catch {
      continue;
    }
    if (!name || !host || !Number.isInteger(port) || port <= 0) continue;
    // Bracket bare IPv6 so hostPort matches location.host and parses in URLs.
    if (host.includes(":") && !host.startsWith("[")) host = `[${host}]`;
    out.push({ id: ids ? ids[i] : null, name, hostPort: `${host}:${port}` });
  }
  return out;
}

// Stable per-entry key for liveness maps and wheel actions: the store ID
// when the shell sent one, else a name@hostPort composite (still unique
// enough for the legacy shell, whose entries can't be told apart by ID).
function charKey(c) {
  return c.id || `${c.name}@${c.hostPort}`;
}

// The vellum:// navigation for picking this entry: ID-addressed when we
// have one; the legacy name form otherwise.
function connectHref(c) {
  return c.id
    ? `vellum://remote/connect?id=${encodeURIComponent(c.id)}`
    : `vellum://remote/connect?name=${encodeURIComponent(c.name)}`;
}

// Find the entry a shell:connect:<key> wheel action refers to; null sends
// the caller to the picker. Exact key match first; a bare-name fallback
// (host-authored wheel configs predating keys) resolves only when it is
// unambiguous.
function findByKey(chars, key) {
  const exact = chars.find((c) => charKey(c) === key);
  if (exact) return exact;
  const byName = chars.filter((c) => c.name === key);
  return byName.length === 1 ? byName[0] : null;
}

const CharCore = { parseShellChars, charKey, connectHref, findByKey };

if (typeof module !== "undefined" && module.exports) {
  module.exports = CharCore;
} else {
  globalThis.CharCore = CharCore;
}

})();
