# Contributing to VellumFE

Contributions are welcome — bug reports, fixes, features, docs, skins, and
layouts alike. The [VellumFE Discord](https://discord.gg/6nKhWRTkSN) is the
best place to discuss an idea before writing code.

## Development

```bash
# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -- --port 8000
```

### VellumFE — Despana checks

Despana's browser unit tests use only Node.js 22's built-in test runner; there
is no npm install step. Run the same focused checks as CI from the repository
root:

```bash
# Session, workspace layout/storage, interactions, map, and font-scale behavior
node --test \
  src/frontend/web/assets/despana/session.test.mjs \
  src/frontend/web/assets/despana/layout.test.mjs \
  src/frontend/web/assets/despana/layout-storage.test.mjs \
  src/frontend/web/assets/despana/interactions.test.mjs \
  src/frontend/web/assets/despana/map.test.mjs \
  src/frontend/web/assets/despana/font-scale.test.mjs

# Feature-independent Rust unit and integration tests
cargo test --no-default-features
```

The end-to-end browser smoke test requires Firefox and geckodriver. With both
executables on `PATH`, run:

```bash
MOZ_HEADLESS=1 node --test \
  src/frontend/web/assets/despana/browser-smoke.test.mjs
```

If either executable is elsewhere, set its absolute path with `FIREFOX` or
`GECKODRIVER` before running the same command.

Please keep pull requests focused (one change per PR) and make sure
`cargo test` passes before submitting.

## Contribution licensing

VellumFE is licensed under the GNU General Public License v3.0 or later
(see [LICENSE](LICENSE)).

By submitting a contribution (pull request, patch, or other material) you
agree that:

1. Your contribution is licensed under the GPL-3.0-or-later, like the rest
   of the project; and
2. You additionally grant the project maintainer (Nisugi) a perpetual,
   worldwide, non-exclusive, royalty-free license to use, modify, and
   distribute your contribution under other license terms.

Point 2 exists so the project can keep shipping builds through channels
with GPL-incompatible distribution terms (such as Apple's App Store /
TestFlight) and grant one-off license exceptions, exactly as is possible
today while the code is single-author. You retain the copyright to your
contribution.
