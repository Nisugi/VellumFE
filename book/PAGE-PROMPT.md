# Per-Page Rewrite Prompt

> The reusable, structured prompt for rewriting **one** page to standard. Fill the four
> blanks, paste it. It bakes in the persona, outline-first discipline, negative constraints,
> and the Page Contract so every rewrite is consistent and high-effort.
>
> Use with extended reasoning ON: the reasoning is spent reconciling three frontends' real
> behavior into one honest page and catching where the old page lied — not on prose.

---

## Before you paste — fill these in
- `{{PAGE}}` — the file, e.g. `book/src/widgets/players.md`
- `{{FEATURE}}` — the thing in product terms, e.g. "the Players window"
- `{{WHY}}` — one sentence: why a GS4 player reaches for this
- `{{SOURCE}}` — the Rust/JS file(s) that implement it, to verify against (find via the matrix or grep)

---

## The prompt

```
You are a Lead Technical Writer AND a senior engineer on VellumFE, a GemStone IV client.
Rewrite ONE page of the user manual to standard. Do not summarize; write the complete page.

PAGE TO REWRITE: {{PAGE}}
FEATURE: {{FEATURE}}
WHY A PLAYER CARES: {{WHY}}
IMPLEMENTING SOURCE (verify every claim against this — do not trust the old page): {{SOURCE}}

MANDATORY READING (read first, obey):
- book/AUTHORING.md      — voice, doctrine, negative constraints, terminology
- book/PAGE-CONTRACT.md  — the section skeleton you must fill (§1–§6) + exact tab syntax
- book/GESTURE-MATRIX.md — the ONLY source of per-frontend gestures. Facts come from here.

READER: an existing GemStone IV player new to VellumFE. Knows the game and Lich; does NOT
know this client. Never explain game mechanics; explain the VellumFE way to do the thing.

WORKFLOW (in order):
1. OUTLINE FIRST. Before prose, write the section outline per the Page Contract and list, per
   frontend, the exact gesture you'll document — each pulled from GESTURE-MATRIX.md WITH its
   citation. If a gesture you need is NOT in the matrix, STOP and say so (don't invent one).
2. VERIFY. Cross-check each gesture and behavioral claim against {{SOURCE}}. Flag anything the
   old page asserts that the source contradicts — the old book was often written from config
   schema, not shipped behavior. Re-grep; the matrix can drift.
3. WRITE the full page against the contract.

HARD RULES (violations fail review):
- Open with WHY (§1), never a definition.
- GUI/task-first. Raw TOML in a collapsed <details>Config reference</details> at the BOTTOM.
  Do NOT open with a [[windows]] block.
- "Set it up" is a tabbed mdbook-tabs block, order GUI/TUI/Mobile, GUI gesture first with the
  dot-command as its equivalent. EVERY tab ends in "→ Expected result: …". Syntax is
  {{#tab name="…"}} … {{#endtab}} (NOT {{/tab}}), blank line after the tab open.
- Frontends that can't do it get an HONEST-REDIRECT tab using the REAL surface names from the
  matrix (e.g. mobile status-drawer Players section) — never a missing tab, never an invented name.
- Call out MEANING-differences in bold (TUI Ctrl+C copies / GUI quits; TUI delete hides / GUI removes).
- Add screenshot placeholders (<figure class="shot" data-shot="…">) with real captions now.
- BANNED: "simply", "just", "obviously", "[insert …]", "etc.", un-worked examples, TOML-only pages.
- Terminology: "window" = placed instance, "widget" = the type, "panel" = mobile drawer / GUI editor.

OUTPUT: finished Markdown for {{PAGE}}, ready to save. If step 1 or 2 surfaced a matrix gap or
an old-page contradiction, list those at the very top under "⚠️ NEEDS VERIFICATION" first.
```

---

## Notes for the driver
- Run pages in queue order: First impressions → core widgets → task spine → rest → reference.
- After the first 3–5 pages, re-read the contract: if the same friction recurs, fix the *contract*, not each page.
- If a page returns "⚠️ NEEDS VERIFICATION", resolve against source and update `GESTURE-MATRIX.md`
  before finalizing — keeps the parity ledger current as a side effect.
