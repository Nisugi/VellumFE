// VellumFE WebUI panel core — draft-edit state and local-notice rules for
// the phone client's Lich WebUI panel, state-free of the DOM and
// app-independent (docs/mobile-runtime-fixes-proposal.md items 11 and 12).
//
// Item 11 (form drafts): renderWebUi rebuilds the panel DOM on every server
// render, so in-progress typing must live OUTSIDE the DOM. The draft store
// keys edits by page + component id (pages are isolated even when cids
// collide), remembers what the user typed (`dirty`), and after a commit
// keeps showing the committed value until the server answers. "The server
// answers" is defined by render sequence: a commit records the page's render
// seq at commit time, and the first render with a NEWER seq is authoritative
// — whether it echoes, normalizes, or rejects the value (WebUI scripts
// re-render after every event, so the event's own render carries a newer
// seq). Renders at or below the commit seq are unrelated in-flight updates
// and must not stomp the field. Drafts are memory-only (passwords included:
// never localStorage, never logged) and are dropped when their component or
// page goes away.
//
// Item 12 (notices): server text dedup is a strictly-increasing sequence
// cursor; local notices have no server sequence and must never touch it
// (a fabricated seq would either be dropped — the old bug: negative seqs
// always failed `seq <= lastSeq` — or suppress legitimate game text and
// corrupt resume). `noticeLine` builds a styled line for the shared
// renderer; the app inserts it through a path that bypasses the sequence
// gate but shares buffers/limits. Policy: local notices live in the client
// text buffers only, so a FULL snapshot (which clears those buffers)
// clears local notices with them; resume/gap snapshots keep them.
//
// Loaded as a classic script BEFORE the app.js module (globalThis.WebuiCore)
// and require()-able from node, so tests exercise the exact shipped bytes.
"use strict";

(function () {

// ---- Item 11: draft-edit store ---------------------------------------------

function createDraftStore() {
  // key -> { value, dirty, pending, commitSeq }
  //   dirty:   user has typed since the last commit; value is theirs.
  //   pending: committed, waiting for a render newer than commitSeq.
  const entries = new Map();
  const key = (page, cid) => `${page}\u0000${cid}`;

  return {
    // User typed into the field.
    setDraft(page, cid, value) {
      entries.set(key(page, cid), { value, dirty: true, pending: false, commitSeq: 0 });
    },

    isDirty(page, cid) {
      const e = entries.get(key(page, cid));
      return !!(e && e.dirty);
    },

    // Blur/Enter commit. Returns the value to emit, or null when nothing
    // should be sent: only a dirty entry can commit, so a re-render (or a
    // second blur) can never duplicate a commit. `alwaysEmit` (passwords)
    // sends even when the text matches the server value.
    commit(page, cid, currentValue, serverValue, pageSeq, alwaysEmit) {
      const k = key(page, cid);
      const e = entries.get(k);
      if (!e || !e.dirty) return null;
      if (!alwaysEmit && currentValue === serverValue) {
        entries.delete(k); // typed back to the server value: nothing to send
        return null;
      }
      entries.set(k, { value: currentValue, dirty: false, pending: true, commitSeq: pageSeq || 0 });
      return currentValue;
    },

    // What the field should show on a render at `renderSeq`. Dirty drafts
    // win; a pending commit holds until a strictly newer render arrives,
    // at which point the server value is authoritative and the entry ends.
    displayValue(page, cid, serverValue, renderSeq) {
      const k = key(page, cid);
      const e = entries.get(k);
      if (!e) return serverValue;
      if (e.dirty) return e.value;
      if (e.pending && (renderSeq || 0) <= e.commitSeq) return e.value;
      entries.delete(k);
      return serverValue;
    },

    // The component disappeared from its page's tree.
    dropComponent(page, cid) {
      entries.delete(key(page, cid));
    },

    // The page closed / unsubscribed: drop everything under it.
    dropPage(page) {
      const prefix = `${page}\u0000`;
      for (const k of [...entries.keys()]) {
        if (k.startsWith(prefix)) entries.delete(k);
      }
    },

    // A fresh tree arrived: keep only drafts whose cid is still present.
    retainComponents(page, liveCids) {
      const prefix = `${page}\u0000`;
      for (const k of [...entries.keys()]) {
        if (k.startsWith(prefix) && !liveCids.has(k.slice(prefix.length))) {
          entries.delete(k);
        }
      }
    },

    size() { return entries.size; },
  };
}

// Collect the cids of every draft-bearing (text-editable) component in a
// WebUI tree, for pruning drafts of removed components.
function collectEditableCids(tree) {
  const out = new Set();
  const walk = (node) => {
    if (!node || typeof node !== "object") return;
    const t = node.t || "";
    if ((t === "text_input" || t === "password_input" || t === "textarea") && node.cid) {
      out.add(node.cid);
    }
    for (const c of node.children || []) walk(c);
  };
  walk(tree);
  return out;
}

const WebuiCore = { createDraftStore, collectEditableCids };

if (typeof module !== "undefined" && module.exports) {
  module.exports = WebuiCore;
} else {
  globalThis.WebuiCore = WebuiCore;
}

})();
