// node --test src/frontend/web/assets/webui-core.test.mjs
//
// WebUI draft-edit and local-notice rules (docs/mobile-runtime-fixes-proposal
// items 11 and 12): drafts surviving unrelated re-renders, commit-once
// semantics, server acknowledgment/normalization, component/page cleanup and
// page isolation; notices bypassing the server-sequence dedup gate without
// moving the resume cursor, and full-snapshot clearing policy.
import test from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const WebuiCore = require("./webui-core.js");

// ---- Item 11: draft store --------------------------------------------------

test("draft survives an unrelated re-render", () => {
  const s = WebuiCore.createDraftStore();
  s.setDraft("p1", "name", "half-typ");
  // Unrelated update re-renders the page at a newer seq; the field is still
  // dirty, so the draft wins over the (unchanged) server value.
  assert.equal(s.displayValue("p1", "name", "old", 7), "half-typ");
  assert.equal(s.displayValue("p1", "name", "old", 8), "half-typ");
  assert.ok(s.isDirty("p1", "name"));
});

test("clean field renders the server value untouched", () => {
  const s = WebuiCore.createDraftStore();
  assert.equal(s.displayValue("p1", "name", "server", 3), "server");
});

test("commit emits once; render-induced blur cannot duplicate it", () => {
  const s = WebuiCore.createDraftStore();
  s.setDraft("p1", "name", "Rysk");
  assert.equal(s.commit("p1", "name", "Rysk", "old", 5, false), "Rysk");
  // A re-render tears down the focused input and fires blur again — the
  // entry is no longer dirty, so the second commit is a no-op.
  assert.equal(s.commit("p1", "name", "Rysk", "old", 5, false), null);
  assert.equal(s.commit("p1", "name", "Rysk", "old", 6, false), null);
});

test("commit with unchanged value emits nothing (non-password)", () => {
  const s = WebuiCore.createDraftStore();
  s.setDraft("p1", "name", "same");
  assert.equal(s.commit("p1", "name", "same", "same", 5, false), null);
  // Entry gone: server value shows again.
  assert.equal(s.displayValue("p1", "name", "same", 5), "same");
});

test("password commit emits even when equal to server value", () => {
  const s = WebuiCore.createDraftStore();
  s.setDraft("p1", "pw", "hunter2");
  assert.equal(s.commit("p1", "pw", "hunter2", "hunter2", 5, true), "hunter2");
});

test("untyped field never commits (no blur spam)", () => {
  const s = WebuiCore.createDraftStore();
  assert.equal(s.commit("p1", "pw", "server", "server", 5, true), null);
});

test("pending commit holds through stale renders, then server acks", () => {
  const s = WebuiCore.createDraftStore();
  s.setDraft("p1", "name", "Rysk");
  s.commit("p1", "name", "Rysk", "old", 5, false);
  // Renders at or below the commit seq (in-flight, unrelated) keep showing
  // the committed value even though the server still says "old".
  assert.equal(s.displayValue("p1", "name", "old", 5), "Rysk");
  // The first NEWER render is authoritative — here the server normalized.
  assert.equal(s.displayValue("p1", "name", "RYSK", 6), "RYSK");
  // Entry consumed.
  assert.equal(s.displayValue("p1", "name", "old", 7), "old");
});

test("server rejection (re-render with old value) is authoritative", () => {
  const s = WebuiCore.createDraftStore();
  s.setDraft("p1", "name", "bad!");
  s.commit("p1", "name", "bad!", "old", 5, false);
  assert.equal(s.displayValue("p1", "name", "old", 6), "old");
});

test("component removal clears its draft", () => {
  const s = WebuiCore.createDraftStore();
  s.setDraft("p1", "gone", "text");
  s.setDraft("p1", "kept", "text2");
  s.retainComponents("p1", new Set(["kept"]));
  assert.equal(s.displayValue("p1", "gone", "sv", 9), "sv");
  assert.equal(s.displayValue("p1", "kept", "sv", 9), "text2");
});

test("dropComponent and dropPage clear drafts", () => {
  const s = WebuiCore.createDraftStore();
  s.setDraft("p1", "a", "x");
  s.setDraft("p1", "b", "y");
  s.setDraft("p2", "a", "z");
  s.dropComponent("p1", "a");
  assert.equal(s.displayValue("p1", "a", "sv", 1), "sv");
  s.dropPage("p1");
  assert.equal(s.displayValue("p1", "b", "sv", 1), "sv");
  // Other page untouched.
  assert.equal(s.displayValue("p2", "a", "sv", 1), "z");
});

test("pages with identical component ids stay isolated", () => {
  const s = WebuiCore.createDraftStore();
  s.setDraft("pageA", "name", "draft-A");
  s.setDraft("pageB", "name", "draft-B");
  assert.equal(s.displayValue("pageA", "name", "sv", 1), "draft-A");
  assert.equal(s.displayValue("pageB", "name", "sv", 1), "draft-B");
  s.dropPage("pageA");
  assert.equal(s.displayValue("pageB", "name", "sv", 1), "draft-B");
  // Page ids that concatenate ambiguously must not collide either.
  s.setDraft("a b", "c", "one");
  assert.ok(!s.isDirty("a", "b c"));
});

test("collectEditableCids finds nested editable components only", () => {
  const cids = WebuiCore.collectEditableCids({
    t: "page",
    children: [
      { t: "text_input", cid: "ti" },
      { t: "button", cid: "btn" },
      { t: "columns", children: [
        { t: "col", children: [{ t: "textarea", cid: "ta" }] },
        { t: "col", children: [{ t: "password_input", cid: "pw" }] },
      ] },
    ],
  });
  assert.deepEqual([...cids].sort(), ["pw", "ta", "ti"]);
});

// ---- Item 12: notices + sequence gate --------------------------------------
//
// Mini-harness mirroring app.js wiring: appendText consults
// acceptServerSeq(lastSeq, seq) and advances the cursor on accept;
// appendLocalLine pushes straight into the buffer without touching either.

function makeFeed(maxLines = 1000) {
  const feed = { lastSeq: 0, lines: [] };
  feed.appendServer = (seq, line) => {
    if (!WebuiCore.acceptServerSeq(feed.lastSeq, seq)) return false;
    feed.lastSeq = seq;
    feed.lines.push(line);
    if (feed.lines.length > maxLines) feed.lines.shift();
    return true;
  };
  feed.appendLocal = (line) => {
    feed.lines.push(line);
    if (feed.lines.length > maxLines) feed.lines.shift();
  };
  return feed;
}

const txt = (l) => l.segments.map((s) => s.text).join("");

test("notice at cursor zero renders and leaves the cursor at zero", () => {
  const feed = makeFeed();
  feed.appendLocal(WebuiCore.noticeLine({ level: "error", text: "boom" }));
  assert.equal(feed.lines.length, 1);
  assert.equal(txt(feed.lines[0]), "[WebUI error] boom");
  assert.equal(feed.lastSeq, 0);
});

test("notice after N server lines renders; cursor unchanged; next seq still lands", () => {
  const feed = makeFeed();
  for (let i = 1; i <= 5; i++) feed.appendServer(i, { segments: [{ text: `line ${i}` }] });
  feed.appendLocal(WebuiCore.noticeLine({ level: "info", text: "hi" }));
  assert.equal(feed.lastSeq, 5);
  assert.equal(feed.lines.length, 6);
  // The next legitimate server sequence is not suppressed by the notice.
  assert.ok(feed.appendServer(6, { segments: [{ text: "line 6" }] }));
  assert.equal(txt(feed.lines.at(-1)), "line 6");
});

test("real server dedup unaffected: duplicate and stale seqs still dropped", () => {
  const feed = makeFeed();
  assert.ok(feed.appendServer(3, { segments: [{ text: "a" }] }));
  feed.appendLocal(WebuiCore.noticeLine({ text: "notice" }));
  assert.ok(!feed.appendServer(3, { segments: [{ text: "dup" }] }));
  assert.ok(!feed.appendServer(2, { segments: [{ text: "stale" }] }));
  assert.equal(feed.lines.length, 2);
});

test("notices share buffer limits with server lines", () => {
  const feed = makeFeed(3);
  for (let i = 1; i <= 3; i++) feed.appendServer(i, { segments: [{ text: `l${i}` }] });
  feed.appendLocal(WebuiCore.noticeLine({ text: "n" }));
  assert.equal(feed.lines.length, 3); // oldest evicted
  assert.equal(txt(feed.lines[0]), "l2");
});

test("notice text with markup stays a plain styled segment", () => {
  const line = WebuiCore.noticeLine({ level: "error", text: '<img src=x onerror=1> & "quotes"' });
  // One plain-text segment; the renderer assigns it via textContent, so the
  // markup can only ever display literally.
  assert.equal(line.segments.length, 1);
  assert.equal(line.segments[0].text, '[WebUI error] <img src=x onerror=1> & "quotes"');
  assert.ok(!("html" in line.segments[0]));
});

test("notice defaults: missing level is info, missing text empty", () => {
  assert.equal(txt(WebuiCore.noticeLine({})), "[WebUI info] ");
  assert.equal(txt(WebuiCore.noticeLine(undefined)), "[WebUI info] ");
});

test("full-snapshot policy: clearing the buffer clears local notices with it", () => {
  const feed = makeFeed();
  feed.appendServer(1, { segments: [{ text: "a" }] });
  feed.appendLocal(WebuiCore.noticeLine({ text: "n" }));
  // app.js full-snapshot path: lastSeq = 0, buffers emptied.
  feed.lastSeq = 0;
  feed.lines.length = 0;
  assert.equal(feed.lines.length, 0);
  // Post-snapshot server text starts clean.
  assert.ok(feed.appendServer(1, { segments: [{ text: "fresh" }] }));
});
