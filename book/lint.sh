#!/usr/bin/env bash
# Book lint — mechanical checks from AUTHORING.md and PAGE-CONTRACT.md.
#
# Structure only: it cannot tell whether a documented gesture is TRUE. That is
# what source citations and spot-checks are for. Run from the repo root or from
# book/:  ./book/lint.sh [--strict]
#
# Default mode checks the rules that apply to EVERY page (orphans, banned words,
# tab syntax, shot captions). --strict additionally requires the full Page
# Contract (tabs/shots/details), which only rewritten pages satisfy — use it once
# a wave lands, not while pages are still stubs.

set -uo pipefail
cd "$(dirname "$0")" || exit 1

STRICT=0
[ "${1:-}" = "--strict" ] && STRICT=1

fail=0
err() { printf '  %s\n' "$1"; fail=1; }

# Pages exempt from the Page Contract, with the reason (a stated decision, per
# the contract preamble — not a silent skip).
is_exempt() {
  case "$1" in
    reference/privacy.md) return 0 ;;                 # legal text
    customization/skin-art-prompts.md) return 0 ;;    # contributor reference
    customization/submitting-art.md) return 0 ;;      # contributor reference
    reference/commands.md|reference/cli.md) return 0 ;;  # command tables
    reference/faq.md|reference/troubleshooting.md) return 0 ;;  # Q&A format
    configuration/*) return 0 ;;                      # the TOML appendix itself
    *) return 1 ;;
  esac
}

# A stub is an honest placeholder awaiting its wave; skip contract checks.
is_stub() { grep -q '^> \*\*Not written yet\.\*\*' "$1"; }

pages=$(find src -name '*.md' ! -name 'SUMMARY.md' | sed 's|^src/||' | sort)

echo "== orphan check (every page reachable from SUMMARY.md) =="
for p in $pages; do
  grep -q "($p)\|(\./$p)" src/SUMMARY.md || err "ORPHAN not in SUMMARY.md: $p"
done

echo "== banned words =="
for p in $pages; do
  f="src/$p"
  if grep -nEi '\b(simply|obviously)\b|\[insert' "$f" | grep -qv '^\s*$'; then
    err "$p: banned word (simply/obviously/[insert)"
  fi
done

echo "== tab syntax =="
for p in $pages; do
  f="src/$p"
  grep -q '{{/tab}}' "$f" && err "$p: uses {{/tab}} — must be {{#endtab}}"
  # Every opened tab must close and must state an Expected result.
  opens=$(grep -c '{{#tab name=' "$f")
  closes=$(grep -c '{{#endtab}}' "$f")
  [ "$opens" -ne "$closes" ] && err "$p: $opens tab opens vs $closes closes"
  if [ "$opens" -gt 0 ]; then
    expected=$(grep -c 'Expected result' "$f")
    [ "$expected" -lt "$opens" ] && err "$p: $opens tabs but only $expected 'Expected result' lines"
  fi
done

echo "== screenshot placeholders have captions =="
for p in $pages; do
  f="src/$p"
  shots=$(grep -c 'data-shot=' "$f")
  if [ "$shots" -gt 0 ]; then
    caps=$(grep -c '<figcaption>' "$f")
    [ "$caps" -lt "$shots" ] && err "$p: $shots shots but only $caps captions"
  fi
done

echo "== duplicate shot IDs =="
dupes=$(grep -rho 'data-shot="[^"]*"' src | sort | uniq -d)
[ -n "$dupes" ] && err "duplicate data-shot IDs: $dupes"


echo "== built HTML has no leaked tab syntax =="
if [ -d book ]; then
  leaked=$(grep -rl '{{#tab\|{{#endtab\|{{#tabs' book 2>/dev/null | head -5)
  [ -n "$leaked" ] && err "leaked tab syntax in built HTML (missing blank line before {{#endtab}}): $leaked"
fi

if [ "$STRICT" = "1" ]; then
  echo "== STRICT: Page Contract compliance =="
  for p in $pages; do
    f="src/$p"
    is_exempt "$p" && continue
    is_stub "$f" && continue
    grep -q '{{#tabs' "$f"        || err "$p: no {{#tabs}} 'Set it up' block"
    grep -q 'class="shot"' "$f"   || err "$p: no screenshot placeholder"
    grep -q '<details>' "$f"      || err "$p: no collapsed config appendix"
    grep -q '## See also' "$f"    || err "$p: no 'See also' section"
    # Banned failure mode: a code fence or TOML table immediately after the H1.
    first=$(grep -vE '^\s*$' "$f" | sed -n '2p')
    case "$first" in
      '```'*|'[['*) err "$p: opens with a code block/TOML — lead with why (AUTHORING §2)" ;;
    esac
  done
fi

if [ "$fail" = "0" ]; then
  echo "OK"
else
  echo "FAILURES above"
fi
exit $fail
