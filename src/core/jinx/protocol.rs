//! The Jinx wire protocol: the manifest shape a repository publishes and the
//! digest used to tell whether a local copy matches it.
//!
//! A repository is a plain static host. `GET {repo}/manifest.json` returns an
//! `available` array; each entry names a `file` (a path appended to the repo
//! URL) and carries a `md5` — which, despite the name, is
//! `base64(SHA1(bytes))` (Jinx's historical field name; the value has always
//! been SHA1). We recompute that digest with [`digest_b64`] and compare, so a
//! file reads as unchanged only when its bytes are byte-for-byte identical to
//! what the repository advertises.
//!
//! VellumFE stays read-compatible with Jinx: manifests we consume from the
//! elanthia-online repos parse here, and the manifests our own repositories
//! publish stay valid Jinx manifests. Our additions ride in an optional
//! [`VellumMeta`] block that Jinx ignores as an unknown key.

use base64::Engine as _;
use serde::Deserialize;
use sha1::{Digest, Sha1};

/// One repository's advertised assets.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub available: Vec<Asset>,
}

/// A single downloadable asset as the manifest describes it.
#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    /// Path appended to the repo URL to fetch the bytes, e.g.
    /// `/skins/parchment.vellumpack`.
    pub file: String,
    /// `script | data | engine | skin | icon | layout | uipack`. Absent means
    /// `script` in Jinx; VellumFE ignores `script`/`engine` entirely.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    /// `base64(SHA1(bytes))` of the delivered file. Named `md5` for Jinx
    /// compatibility; the value is SHA1.
    pub md5: String,
    /// Unix epoch of the file's last commit, for "modified N ago".
    #[serde(default)]
    pub last_commit: i64,
    /// Optional path to a documentation excerpt.
    #[serde(default)]
    pub header: Option<String>,
    /// VellumFE-only enrichment. Jinx never writes or reads this; our
    /// repositories populate it from a contributor's `meta.toml`.
    #[serde(default)]
    pub vellum: Option<VellumMeta>,
}

/// The optional VellumFE superset block: what the GUI gallery renders and what
/// `installed.toml` records for version-aware updates.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct VellumMeta {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    /// Path *inside* the asset bundle to a preview image.
    #[serde(default)]
    pub preview: Option<String>,
}

impl Asset {
    /// The asset's plain filename (the last path segment of `file`), which is
    /// how a user names it on the command line and how it's stored on disk.
    pub fn basename(&self) -> &str {
        let trimmed = self.file.trim_end_matches('/');
        match trimmed.rsplit_once('/') {
            Some((_, name)) => name,
            None => trimmed,
        }
    }

    /// The resolved asset kind, defaulting to `script` exactly as Jinx does
    /// for a missing `type`.
    pub fn kind(&self) -> &str {
        self.kind.as_deref().unwrap_or("script")
    }
}

/// `base64(SHA1(bytes))` — byte-for-byte identical to Ruby's
/// `Digest::SHA1.base64digest`, which is what the repository generator and
/// Jinx itself compute. This is the one function on which change-detection
/// parity rests; do not "optimize" the algorithm.
pub fn digest_b64(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The digest MUST match Ruby's `Digest::SHA1.base64digest`. These vectors
    /// were produced by the repository generator prototype (build_manifest.rb)
    /// and cross-checked against an independent SHA1-b64; if this test ever
    /// fails, the client and every published manifest have silently diverged.
    #[test]
    fn digest_matches_jinx_ruby_sha1_base64() {
        // `printf '<xml>gameobj</xml>'` — the data-file vector from the
        // generator self-test.
        assert_eq!(digest_b64(b"<xml>gameobj</xml>"), "7qt1tdPIzApVUQB0BpOKxeV3X4w=");
        // The empty input is a stable, well-known SHA1-b64 constant.
        assert_eq!(digest_b64(b""), "2jmj7l5rSw0yVb/vlWAYkK/YBwk=");
    }

    #[test]
    fn manifest_parses_bare_jinx_entry() {
        // A manifest from an existing elanthia-online repo: no `vellum` block,
        // `type` may be absent (defaults to script).
        let json = r#"{
            "available": [
                { "file": "/go2.lic", "md5": "abc=", "last_commit": 1699999999 },
                { "file": "/data/effect-list.xml", "type": "data",
                  "md5": "def=", "last_commit": 1700000000,
                  "header": "/effect-list.header" }
            ]
        }"#;
        let m: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.available.len(), 2);
        assert_eq!(m.available[0].basename(), "go2.lic");
        assert_eq!(m.available[0].kind(), "script"); // missing type -> script
        assert!(m.available[0].vellum.is_none());
        assert_eq!(m.available[1].kind(), "data");
        assert_eq!(m.available[1].header.as_deref(), Some("/effect-list.header"));
    }

    #[test]
    fn manifest_parses_vellum_superset_entry() {
        // A VellumFE repo entry: full `vellum` block; unknown-to-Jinx but
        // parsed here.
        let json = r#"{
            "available": [
                { "file": "/skins/parchment.vellumpack", "type": "skin",
                  "md5": "qE8VSSxlLSc9VZeSfAEknEYHqKE=", "last_commit": 1785328567,
                  "vellum": { "title": "Parchment", "author": "Nisugi",
                              "description": "Warm aged-paper theme.",
                              "version": "1.2.0", "tags": ["warm", "fantasy"],
                              "preview": "preview.png", "category": "skin" } }
            ]
        }"#;
        let m: Manifest = serde_json::from_str(json).unwrap();
        let a = &m.available[0];
        assert_eq!(a.basename(), "parchment.vellumpack");
        assert_eq!(a.kind(), "skin");
        let v = a.vellum.as_ref().expect("vellum block");
        assert_eq!(v.title.as_deref(), Some("Parchment"));
        assert_eq!(v.version.as_deref(), Some("1.2.0"));
        assert_eq!(v.tags, ["warm", "fantasy"]);
        assert_eq!(v.category.as_deref(), Some("skin"));
    }

    #[test]
    fn unknown_manifest_keys_are_ignored() {
        // Forward-compatibility: a future field must not break parsing.
        let json = r#"{ "available": [
            { "file": "/x", "md5": "z=", "future_field": 42 }
        ], "repo_note": "hi" }"#;
        let m: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.available.len(), 1);
    }
}
