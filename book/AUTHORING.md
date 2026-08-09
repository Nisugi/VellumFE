# Authoring Guide — VellumFE User Manual

> The rulebook for writing this book. Every page obeys these rules so the manual reads as
> one voice and never lies about the product. If a rule here fights a page, the rule wins.
>
> Companion files:
> - [`GESTURE-MATRIX.md`](./GESTURE-MATRIX.md) — source-verified per-frontend gestures. Facts come from here.
> - [`PAGE-CONTRACT.md`](./PAGE-CONTRACT.md) — the mandatory section skeleton every page fills.
> - [`PAGE-PROMPT.md`](./PAGE-PROMPT.md) — the reusable fill-in prompt for rewriting one page.

---

## 1. The Reader Contract

**Our reader is an existing GemStone IV player who is new to VellumFE.**

They know: the game (roundtime, spells, stances, hunting), Lich (they probably launch
through it), their terminal/OS basics.

They do NOT know: anything about *this client* — its windows, dot-commands, editors, skins,
or which of three frontends they're on.

**So:** never explain game concepts (don't define roundtime). Always explain *the VellumFE
way* to do a thing the player already wants. The value we add is "here's how you do the
thing you already know you want, in this client."

## 2. The Doctrine — GUI / task-first, TOML as appendix

The product philosophy is **you never hand-edit TOML** — every feature ships an in-app
editor, and the GUI now has stay-open **Windows / Settings / Zones / Editors** hubs in the
toolbar. The book must match that.

- **Lead with the goal**, in the player's words ("make your health bar flash when low").
- **Then the gesture** — the exact click / command / tap, per frontend (§3).
- **Raw TOML is reference, not instruction.** It lives in a collapsed `<details>` at the
  page **bottom**, for tinkerers and troubleshooting — never above the fold.

If you catch yourself opening a page with a `[[windows]]` block, stop — that's the old book's mistake.

## 3. Per-frontend parity — tabbed, honest, verified

Every "how do I do this" is a **tabbed block** (`mdbook-tabs`), tab order **Desktop GUI ·
Terminal (TUI) · Mobile**. Rules:

1. **Facts come from [`GESTURE-MATRIX.md`](./GESTURE-MATRIX.md), and the matrix outranks your
   intuition.** Never invent a gesture. If a gesture *sounds* obvious ("surely you right-click
   to add a widget"), that's the danger signal — check source. The plausible-but-false gesture
   is the #1 documented failure mode. **Re-grep the cited source at write-time** — the matrix
   can drift when the product changes.
2. **Lead each tab with the GUI-native gesture; show the dot-command as the equivalent.**
   (Dot-commands work in the GUI input too.)
3. **State meaning-differences, not just key-differences, in bold.** Example: in the TUI
   `Ctrl+C` *copies*; in the GUI it *quits*. `.deletewindow` truly removes in the GUI but only
   hides in the TUI. This is the #1 way a tabbed page silently misleads.
4. **Every tab ends with an Expected result** — "→ the panel appears listing your active spells."
5. **Honest redirect for "not available."** When a frontend genuinely can't do it (mostly
   Mobile's fixed chrome), the tab is NOT omitted and NOT a curt "no." It says what happens
   instead and where the equivalent lives — using the REAL surface names from the matrix
   (e.g. the mobile **status drawer**'s **Players** section), never an invented one.
6. Tab order is always Desktop GUI, Terminal (TUI), Mobile.

## 4. Voice

- **Second person, active, present.** "Click the **Windows** button and show **Players**."
- **One idea per sentence.** A player skims mid-hunt.
- **Every widget/feature page opens with a one-line "why you'd reach for this"** — the payoff,
  in game terms. This is the biggest fix for "the book falls flat." A page that opens by
  *declaring what a thing is* is dead on arrival.
- **UI labels in bold, exactly as they appear** (verify the exact label in source — don't
  guess): **Windows** (toolbar), **↩ Restore deleted…**, **Edit Window…**. Commands and paths in `code`.
- Cross-link liberally with real links.

## 5. Negative constraints (what NOT to do)

- ❌ No placeholders in prose — no `[insert steps]`, no "etc.", no "and so on".
- ❌ No un-worked examples. Every example ends in a concrete visible outcome.
- ❌ No page that is only a TOML block + an ASCII box + bullets. That's the banned failure mode.
- ❌ No inventing UI. Unsure a button/menu/surface exists? Verify in source; if you can't, don't write it.
- ❌ No explaining game mechanics the player already knows.
- ❌ No "simply" / "just" / "obviously".

## 6. Screenshots — placeholders now, images later

Every GUI-capable feature needs a visual. Write captions and reserve slots now; PNGs drop in
later. Plain HTML/CSS placeholder (no build tooling):

```html
<figure class="shot" data-shot="gui/players-windows-catalog">
  <div class="shot-ph">📷 screenshot pending</div>
  <figcaption>The <b>Windows</b> toolbar catalog open, with <b>Players</b> in the list.</figcaption>
</figure>
```

- `data-shot` = the shot ID / intended path, grep-able for the shot list:
  `grep -rho 'data-shot="[^"]*"' book/src | sort -u`
- Caption is written **now** — real documentation before the image exists.
- Group by frontend in the prefix (`gui/`, `tui/`, `mobile/`).
- **Screenshots are a per-page byproduct** — the shot list emerges as each page is authored.
  Do not try to enumerate needed screenshots before the pages exist.
- CSS lives in `book/theme/shot.css`, pulled in via `additional-css`. See [`PAGE-CONTRACT.md`](./PAGE-CONTRACT.md).

## 7. Versioning & honesty about maturity

- Mark experimental/beta surfaces inline: "**(beta — via TestFlight)**", "**(experimental)**".
- iOS = TestFlight beta; Android = in progress; Mobile web = two modes (sidecar / headless).
- GUI-only / not-yet-on-mobile is carried by the honest-redirect tab (§3.5).

## 8. Terminology — one name per thing

- **Window** — a bordered, placeable container in TUI/GUI (health bar, main text). Use this for the placed instance.
- **Widget** — the *type* that fills a window (`progress`, `compass`). Use only for the type/kind.
- **Panel** — reserve for **Mobile** drawers/sheets and GUI **editor** panels. Not a game window.
- **Editor** — an in-app configuration UI (Settings, Highlights, Window editor).
- Mobile surfaces have exact names — see the matrix's "Mobile UI surface names" block. Use those; never invent (no "Players chip").

## 9. Status & backlog

**Restarted fresh 2026-08-09.** Prior (uncommitted) framework files were lost; rebuilt on
current post-menu-overhaul source. Confirmed decisions: audience = GS4 player new to Vellum;
doctrine = GUI/task-first, TOML appendix; structure = GUI/TUI/Mobile tabs + honest-redirect.

Living list; add as we go:
- [ ] **How-To Guides** task-spine section (new): "Build a combat layout", "Make vitals flash
  on low health", "Set up highlights & sounds". Highest-leverage structural add — the book has
  no task-oriented entry point today.
- [ ] Accessibility note slot per page (TTS / screen-reader). [[accessibility-first-gui-customization]]
- [ ] Glossary page seeded from §8.
- [ ] **Doc-bug to fix:** `web.md:164` claims the mobile highlight editor can't do
  redirects/squelch — current code DOES (matrix [doc-bug] row). Correct when web.md is touched.
- [ ] **Open product question:** delete-vs-hide — collapse to Hide-only? (matrix [repair?] row.)
  Keep those docs minimal until decided.
- [ ] Consider `.gitignore` for generated `book/book/` output (churns on every build).
