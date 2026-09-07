// VellumFE dashboard pairing core — token/fragment rules and session-list
// classification for dashboard.html, state-free and DOM-independent
// (docs/mobile-runtime-fixes-proposal.md item 9).
//
// Both native shells open a saved remote server at `/`; a manual entry may
// have been saved WITHOUT a pairing token, and /sessions is token-gated.
// This module lets the dashboard tell the three failure worlds apart —
// denied access (403 / {"error":"forbidden"}), an authenticated but empty
// session list, and the server not answering at all — so denied access is
// never rendered as "No sessions running", and pairing can complete in the
// page itself.
//
// Token ownership:
//  - Native app shells (fragment carries app=1) are the authority for a
//    saved server's token. They append `sid=<stable store ID>` to the
//    fragment; a token accepted here round-trips back through
//    vellum://remote/token?id=…&token=… and updates exactly that saved
//    entry — by ID, never by name/host guessing.
//  - Plain browsers have no native store; the dashboard keeps its accepted
//    token in localStorage for reuse on a later visit. The fragment
//    (existing #token=… mechanism) remains the only way a token travels
//    through navigation — never ordinary query strings.
//
// Loaded as a classic script by dashboard.html (globalThis.PairingCore)
// and require()-able from node, so tests exercise the exact shipped bytes.
"use strict";

(function () {

// Pairing token from a location.hash string ("" when absent/undecodable).
function pairingToken(hash) {
  const m = (hash || "").match(/(?:^#|&)token=([^&]+)/);
  if (!m) return "";
  try {
    return decodeURIComponent(m[1]);
  } catch {
    return "";
  }
}

// The native shell's saved-target stable store ID (`sid=`), or null. Only
// app shells send this; it is the address for vellum://remote/token.
function shellSid(hash) {
  const m = (hash || "").match(/(?:^#|&)sid=([^&]+)/);
  if (!m) return null;
  try {
    const d = decodeURIComponent(m[1]);
    return d || null;
  } catch {
    return null;
  }
}

// True when a native app shell loaded the page (it marks the fragment).
function isAppShell(hash) {
  return /(?:^#|&)app=1(?:&|$)/.test(hash || "");
}

// Rebuild a fragment with `token=` set to the given token, preserving every
// other param (app/nativepicker/chars/charids/sid/…). Always returns a
// string starting with "#". An empty token leaves the hash untouched.
function withToken(hash, token) {
  const h = hash || "";
  if (!token) return h || "#";
  const enc = "token=" + encodeURIComponent(token);
  const stripped = h
    .replace(/^#/, "")
    .split("&")
    .filter((p) => p && !/^token=/.test(p));
  return "#" + [enc].concat(stripped).join("&");
}

// The vellum:// navigation that hands an accepted token back to the native
// shell's saved entry — addressed by stable store ID. Null when there is
// no sid (plain browser, or an older shell): the token then lives only in
// the page's own storage.
function tokenUpdateUrl(sid, token) {
  if (!sid || !token) return null;
  return (
    "vellum://remote/token?id=" +
    encodeURIComponent(sid) +
    "&token=" +
    encodeURIComponent(token)
  );
}

// Fetch and classify the session list. `fetchFn` is injected (tests stub
// it; the page passes window.fetch). Returns { state, sessions } with
// state one of:
//   "sessions"  — authenticated, at least one entry
//   "empty"     — authenticated, genuinely no sessions
//   "forbidden" — the server answered but rejected the token
//   "network"   — no usable answer (unreachable, non-JSON, 5xx, …)
async function loadSessions(fetchFn, token) {
  let res;
  try {
    res = await fetchFn(
      "/sessions?token=" + encodeURIComponent(token || "")
    );
  } catch {
    return { state: "network", sessions: [] };
  }
  if (res.status === 403) return { state: "forbidden", sessions: [] };
  if (!res.ok) return { state: "network", sessions: [] };
  let body;
  try {
    body = await res.json();
  } catch {
    return { state: "network", sessions: [] };
  }
  if (Array.isArray(body)) {
    return body.length
      ? { state: "sessions", sessions: body }
      : { state: "empty", sessions: [] };
  }
  // Defensive: an older server shape answering 200 {"error":"forbidden"}.
  if (body && body.error === "forbidden") {
    return { state: "forbidden", sessions: [] };
  }
  return { state: "network", sessions: [] };
}

const PairingCore = {
  pairingToken,
  shellSid,
  isAppShell,
  withToken,
  tokenUpdateUrl,
  loadSessions,
};

if (typeof module !== "undefined" && module.exports) {
  module.exports = PairingCore;
} else {
  globalThis.PairingCore = PairingCore;
}

})();
