//! The asset manager: a native, federated client for the Jinx repository
//! protocol. It downloads and keeps current the files that shape the game and
//! the interface — game data (item/spell databases, the map), skins, icon
//! sets, and shareable layouts — without a Lich install.
//!
//! VellumFE speaks Jinx's exact wire protocol ([`protocol`]), so it reads the
//! existing elanthia-online repositories for game data *and* VellumFE's own
//! repositories for skins/icons/layouts. Nothing downloads on its own; every
//! install is a user action (or an opt-in `auto-update`).
//!
//! Structure (built out across phases):
//!   - [`protocol`]  — manifest/asset types + the SHA1-b64 digest (J0)
//!   - [`installer`] — verified single-file download + atomic write (J0)
//!   - repo/manifest/metadata/hooks/worker + the `.jinx` command surface follow.
//!
//! Design and phasing live in `.claude/work-units/jinx-native-client-plan.md`.

pub mod installer;
pub mod protocol;
