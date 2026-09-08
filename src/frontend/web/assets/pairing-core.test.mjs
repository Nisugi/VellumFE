// node --test src/frontend/web/assets/pairing-core.test.mjs
//
// Dashboard pairing rules (docs/mobile-runtime-fixes-proposal.md item 9):
// denied access must never classify as an empty session list; token
// fragment threading; the vellum://remote/token round-trip address.
import test from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const PairingCore = require("./pairing-core.js");

// A fetch stub returning one canned response.
function fetchStub({ status = 200, body, reject = false, badJson = false }) {
  return async () => {
    if (reject) throw new TypeError("network down");
    return {
      ok: status >= 200 && status < 300,
      status,
      json: async () => {
        if (badJson) throw new SyntaxError("not json");
        return body;
      },
    };
  };
}

// ---- loadSessions classification ------------------------------------------

test("missing token: 403 classifies as forbidden, not empty", async () => {
  const r = await PairingCore.loadSessions(
    fetchStub({ status: 403, body: { error: "forbidden" } }),
    ""
  );
  assert.deepEqual(r, { state: "forbidden", sessions: [] });
});

test("rejected token classifies as forbidden", async () => {
  const r = await PairingCore.loadSessions(
    fetchStub({ status: 403, body: { error: "forbidden" } }),
    "wrong-token"
  );
  assert.equal(r.state, "forbidden");
});

test("200 {\"error\":\"forbidden\"} body still classifies as forbidden", async () => {
  const r = await PairingCore.loadSessions(
    fetchStub({ status: 200, body: { error: "forbidden" } }),
    ""
  );
  assert.equal(r.state, "forbidden");
});

test("successful retry returns the session list", async () => {
  const sessions = [{ character: "Rysk", port: 8001 }];
  const r = await PairingCore.loadSessions(
    fetchStub({ status: 200, body: sessions }),
    "good-token"
  );
  assert.deepEqual(r, { state: "sessions", sessions });
});

test("authenticated empty list is 'empty', distinct from forbidden", async () => {
  const r = await PairingCore.loadSessions(
    fetchStub({ status: 200, body: [] }),
    "good-token"
  );
  assert.deepEqual(r, { state: "empty", sessions: [] });
});

test("network failure (fetch throws) is 'network'", async () => {
  const r = await PairingCore.loadSessions(fetchStub({ reject: true }), "t");
  assert.deepEqual(r, { state: "network", sessions: [] });
});

test("server error (500) and non-JSON classify as 'network'", async () => {
  assert.equal(
    (await PairingCore.loadSessions(fetchStub({ status: 500, body: {} }), "t"))
      .state,
    "network"
  );
  assert.equal(
    (await PairingCore.loadSessions(fetchStub({ status: 200, badJson: true }), "t"))
      .state,
    "network"
  );
});

test("token is percent-encoded onto the /sessions query", async () => {
  let seen;
  await PairingCore.loadSessions(async (url) => {
    seen = url;
    return { ok: true, status: 200, json: async () => [] };
  }, "a&b=c");
  assert.equal(seen, "/sessions?token=a%26b%3Dc");
});

// ---- fragment parsing ------------------------------------------------------

test("pairingToken reads and decodes the fragment token", () => {
  assert.equal(PairingCore.pairingToken("#token=abc%2F1&app=1"), "abc/1");
  assert.equal(PairingCore.pairingToken("#app=1"), "");
  assert.equal(PairingCore.pairingToken(""), "");
});

test("shellSid reads the shell's stable store ID; absent means null", () => {
  assert.equal(PairingCore.shellSid("#app=1&sid=id%2D9&chars=x@h:1"), "id-9");
  assert.equal(PairingCore.shellSid("#app=1"), null);
});

test("isAppShell only matches the shell marker exactly", () => {
  assert.ok(PairingCore.isAppShell("#token=t&app=1&sid=x"));
  assert.ok(!PairingCore.isAppShell("#token=t"));
  assert.ok(!PairingCore.isAppShell("#app=12"));
});

// ---- token threading (fragment mechanism, never query strings) -------------

test("withToken adds a token while preserving every other param", () => {
  assert.equal(
    PairingCore.withToken("#app=1&nativepicker=1&sid=s1", "tok"),
    "#token=tok&app=1&nativepicker=1&sid=s1"
  );
});

test("withToken replaces a rejected token in place", () => {
  const h = PairingCore.withToken("#token=old&app=1", "new one");
  assert.equal(h, "#token=new%20one&app=1");
  assert.equal(PairingCore.pairingToken(h), "new one");
});

test("withToken on an empty hash still yields a fragment", () => {
  assert.equal(PairingCore.withToken("", "t"), "#token=t");
  assert.equal(PairingCore.withToken("#a=1", ""), "#a=1");
});

// ---- native round-trip address ---------------------------------------------

test("tokenUpdateUrl addresses the saved entry by stable ID", () => {
  assert.equal(
    PairingCore.tokenUpdateUrl("id a", "tok&1"),
    "vellum://remote/token?id=id%20a&token=tok%261"
  );
});

test("tokenUpdateUrl is null without a sid (web-side ownership)", () => {
  assert.equal(PairingCore.tokenUpdateUrl(null, "tok"), null);
  assert.equal(PairingCore.tokenUpdateUrl("id", ""), null);
});
