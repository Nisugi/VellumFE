// node --test src/frontend/web/assets/char-core.test.mjs
//
// Character-switch identity rules (docs/mobile-runtime-fixes-proposal.md
// item 7): fragment decoding with and without charids=, duplicate labels
// at different endpoints staying distinct, independent liveness keys, and
// stale-key/legacy-name resolution.
import test from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const CharCore = require("./char-core.js");

test("parses chars= with parallel charids=", () => {
  const chars = CharCore.parseShellChars(
    "#token=t&app=1&chars=Rysk@10.0.0.5:8000,Rysk@10.0.0.9:8000&charids=id-a,id-b"
  );
  assert.deepEqual(chars, [
    { id: "id-a", name: "Rysk", hostPort: "10.0.0.5:8000" },
    { id: "id-b", name: "Rysk", hostPort: "10.0.0.9:8000" },
  ]);
});

test("legacy fragment without charids yields null ids", () => {
  const chars = CharCore.parseShellChars("#chars=Niffy@host:8000");
  assert.deepEqual(chars, [{ id: null, name: "Niffy", hostPort: "host:8000" }]);
});

test("percent-encoded names/hosts and IPv6 bracketing decode", () => {
  const chars = CharCore.parseShellChars(
    "#chars=A%20B@fe80%3A%3A1:9000&charids=x%2Fy"
  );
  assert.deepEqual(chars, [{ id: "x/y", name: "A B", hostPort: "[fe80::1]:9000" }]);
});

test("mismatched charids length is discarded, entries keep null ids", () => {
  const chars = CharCore.parseShellChars(
    "#chars=A@h:1,B@h:2&charids=only-one"
  );
  assert.equal(chars.length, 2);
  assert.equal(chars[0].id, null);
  assert.equal(chars[1].id, null);
});

test("malformed entries are skipped without desyncing ids", () => {
  // Entry 1 is malformed (no @) — entry 2 must still get ITS id, not id 1's.
  const chars = CharCore.parseShellChars(
    "#chars=nohostport,B@h:2&charids=id-1,id-2"
  );
  assert.deepEqual(chars, [{ id: "id-2", name: "B", hostPort: "h:2" }]);
});

test("charKey: id when present, name@hostPort composite otherwise", () => {
  assert.equal(CharCore.charKey({ id: "abc", name: "R", hostPort: "h:1" }), "abc");
  assert.equal(CharCore.charKey({ id: null, name: "R", hostPort: "h:1" }), "R@h:1");
});

test("duplicate labels get independent liveness keys", () => {
  const [a, b] = CharCore.parseShellChars(
    "#chars=Rysk@h1:8000,Rysk@h2:8000&charids=id-a,id-b"
  );
  const status = {};
  status[CharCore.charKey(a)] = "online";
  status[CharCore.charKey(b)] = "offline";
  assert.equal(status[CharCore.charKey(a)], "online");
  assert.equal(status[CharCore.charKey(b)], "offline");
});

test("connectHref: id-addressed when known, legacy name form otherwise", () => {
  assert.equal(
    CharCore.connectHref({ id: "a b", name: "R", hostPort: "h:1" }),
    "vellum://remote/connect?id=a%20b"
  );
  assert.equal(
    CharCore.connectHref({ id: null, name: "R y", hostPort: "h:1" }),
    "vellum://remote/connect?name=R%20y"
  );
});

test("findByKey: exact key wins; deleted key resolves to null", () => {
  const chars = CharCore.parseShellChars(
    "#chars=Rysk@h1:8000,Rysk@h2:8000&charids=id-a,id-b"
  );
  assert.equal(CharCore.findByKey(chars, "id-b"), chars[1]);
  assert.equal(CharCore.findByKey(chars, "id-gone"), null);
});

test("findByKey: bare-name fallback only when unambiguous", () => {
  const dup = CharCore.parseShellChars(
    "#chars=Rysk@h1:8000,Rysk@h2:8000&charids=id-a,id-b"
  );
  assert.equal(CharCore.findByKey(dup, "Rysk"), null); // ambiguous
  const one = CharCore.parseShellChars("#chars=Niffy@h1:8000&charids=id-a");
  assert.equal(CharCore.findByKey(one, "Niffy"), one[0]); // unique
});
