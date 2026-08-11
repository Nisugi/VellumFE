# Page Contract

> The mandatory skeleton every content page fills. A page that satisfies the contract
> physically cannot fall into the old "TOML block + ASCII box + bullets" trap.
>
> Widget and feature pages must have all of §1–§6. A pure reference page may skip "Set it
> up". Skipping a section is a decision you state, not a default.

---

## The skeleton

```markdown
# <Feature, in product terms>

> One line: what it is **and why you'd reach for it**, in the player's words.

## What it's for            ← §1  the payoff. 2–4 sentences, game terms. Why the player cares.

## Set it up                ← §2  tabbed per-frontend gesture (GUI / TUI / Mobile), each ending in → Expected result.

## Common setups            ← §3  1–2 worked recipes, each ending in a concrete visible outcome.

## Tips & gotchas           ← §4  what goes wrong + how to avoid it. Callouts for meaning-differences.

## See also                 ← §5  real cross-links.

<details>
<summary>Config reference (TOML)</summary>   ← §6  every field, type, default. Tinkerers/troubleshooting only.
</details>
```

## Section rules

### §1 — What it's for
Opens with the payoff, never a definition.
❌ "The Players widget lists players in the room."
✅ "See who else is in the room at a glance — and click a name to interact without typing."

### §2 — Set it up (tabbed)
- `mdbook-tabs` block. Tab order: **Desktop GUI, Terminal (TUI), Mobile**.
- Facts from [`GESTURE-MATRIX.md`](./GESTURE-MATRIX.md) only. GUI gesture first, dot-command as equivalent.
- **Every tab ends with `→ Expected result: …`.**
- **Honest redirect** for any frontend that can't do it — real surface names, never omit the tab.
- **Bold any meaning-difference** (Ctrl+C copies in TUI / quits in GUI; delete hides in TUI / removes in GUI).

**Exact syntax (verified against `mdbook-tabs` v1.0.4):** block is `{{#tabs}}` … `{{#endtabs}}`;
each tab is `{{#tab name="…"}}` … `{{#endtab}}` (it is `{{#endtab}}`, NOT `{{/tab}}`). A **blank
line** is required after `{{#tab …}}` before content. `global="frontend"` makes all blocks on a
page switch together.

> ⚠️ **A blank line is also required BEFORE `{{#endtab}}` when the tab's last
> content is a raw HTML block** — most often a screenshot `</figure>`. Without
> it the preprocessor stops parsing and the literal `{{#tab …}}` text leaks into
> the built page. Found the hard way in Wave 1; verify by grepping the built
> HTML for `{{#` (must be zero) after every build.

    ## Set it up

    {{#tabs global="frontend"}}
    {{#tab name="Desktop GUI"}}

    1. Click the **Windows** button in the top toolbar and tick **Players** in the catalog.
       (Or type `.addwindow players` in the command input.)

    → **Expected result:** a Players window appears, listing everyone in the room.
    {{#endtab}}
    {{#tab name="Terminal (TUI)"}}

    1. Type `.addwindow players` — or `.addwindow` for the picker, then choose **players**.

    → **Expected result:** the Players window appears at the default anchor.
    {{#endtab}}
    {{#tab name="Mobile"}}

    The phone uses a fixed layout, so you don't place this window yourself — the room's
    players appear in the **status drawer**'s **Players** section (open the right drawer, or
    use the touch wheel's **Players** slice). Configure how it looks on desktop.
    {{#endtab}}
    {{#endtabs}}

### §3 — Common setups
1–2 recipes tied to real play. Each ends in an outcome the reader can *see*, not "you're all set."

### §4 — Tips & gotchas
Failure modes from the matrix. Blockquote callouts:
`> ⚠️ **In the TUI, `Ctrl+C` copies your selection — it does not quit.** Use `.quit` to exit.`

### §5 — See also
Real markdown links. Related widgets, relevant config page, any how-to guide.

### §6 — Config reference (collapsed)
Wrapped in `<details><summary>Config reference (TOML)</summary> … </details>`. Full field table:
name · type · default · what it does. Never the primary instruction — if a reader needs this to
do the basic task, §2 failed.

## Variant skeleton — How-To guides

How-to pages are **goal-shaped, not feature-shaped**: the reader arrives wanting
an outcome ("my health bar should flash when I'm hurt"), not a tour of a widget.
They cut across several features, so they use this variant instead of §1–§6:

```markdown
# <The goal, in the player's words>

> One line: what you'll have when you're done.

## What you'll build      ← the finished state, concretely. A sentence or two + a screenshot placeholder.
## Before you start       ← prerequisites only if real (a GS4-only widget, TTS enabled, Lich running). Omit if none.
## Steps                  ← tabbed per-frontend, numbered, each tab ending in → Expected result.
## Make it yours          ← 2-3 variations on the finished setup ("do this instead if you hunt in melee").
## When it doesn't work   ← the specific failure modes of THIS task, with the fix.
## See also               ← the feature pages behind each step.
```

Rules that still apply: GUI-first tabs in the same order, every tab ends in an
Expected result, honest-redirect for frontends that can't do it, no banned words,
screenshot placeholders with real captions.

Rules that do **not** apply: there is no §6 TOML appendix — a how-to links to the
feature page's appendix instead of repeating it. Never make a how-to the only
place a setting is documented; it teaches a path, the feature page owns the facts.

## Screenshots
Placeholder wherever a visual helps (every §2 GUI tab, the frontends overview):

    <figure class="shot" data-shot="gui/players-windows-catalog">
      <div class="shot-ph">📷 screenshot pending</div>
      <figcaption>The <b>Windows</b> toolbar catalog open, with <b>Players</b> in the list.</figcaption>
    </figure>

## The CSS (one-time setup, `book/theme/shot.css`)

```css
.shot { margin: 1.2rem 0; }
.shot-ph {
  border: 2px dashed var(--fg, #888);
  border-radius: 6px;
  padding: 2.5rem 1rem;
  text-align: center;
  opacity: 0.6;
  font-style: italic;
}
.shot figcaption { text-align: center; opacity: 0.8; font-size: 0.9em; margin-top: 0.4rem; }
```

Wire it in `book.toml`: `additional-css = ["theme/tabs.css", "theme/shot.css"]`.

## Definition of done (self-check before a page ships)
- [ ] Opens with a why, not a definition (§1).
- [ ] `Set it up` is tabbed, GUI-first, every tab ends in an Expected result (§2).
- [ ] Every frontend that can't do it has an honest-redirect tab with real surface names, not a missing one (§2).
- [ ] Meaning-differences called out in bold/callout (§4).
- [ ] At least one worked recipe ending in a visible outcome (§3).
- [ ] TOML collapsed at the bottom, not above the fold (§6).
- [ ] No banned words, no `[insert]`, no "etc.".
- [ ] Every gesture matches [`GESTURE-MATRIX.md`](./GESTURE-MATRIX.md); anything new was verified in source and added there first.
- [ ] Screenshot placeholders added with real captions.
