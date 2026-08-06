//! Window-system redesign Phase 0: characterization of the template
//! catalog (spec: .beads/artifacts/window-system-redesign/spec.md).
//!
//! These tests pin the CURRENT behavior of `src/config/templates.rs` as a
//! golden fixture — the equivalence oracle for the strangler-fig
//! migration. Phase 1 introduces a resolver as a pure façade over the
//! catalog; every later swap is proven by these snapshots staying green.
//! Nothing here asserts the behavior is *right* — only that it doesn't
//! change unannounced.
//!
//! Regenerating after a DELIBERATE template change:
//!   UPDATE_GOLDEN=1 cargo test --test template_characterization
//! then review the fixture diff like code and commit it.

use std::fmt::Write as _;
use vellum_fe::config::{Config, GameType};

const GOLDEN_PATH: &str = "tests/fixtures/template_characterization.txt";

/// The wire-verified id inventory (11.4 GB Lich-log sweep) plus negatives:
/// every dialog/stream id whose routing the redesign must preserve, and a
/// few ids that must NOT match anything (per-entity injuries, UberBar,
/// unknown).
const PROBE_IDS: &[&str] = &[
    // dedicated-widget streams
    "inv",
    "reserve",
    "room",
    "Spells",
    // text streams
    "thoughts",
    "speech",
    "familiar",
    "voln",
    "logons",
    "death",
    "combat",
    "charprofile",
    // dialogs with dedicated views
    "minivitals",
    "expr",
    "encum",
    "stance",
    "injuries",
    "IconBAR",
    "compass",
    // effect dialogs
    "Buffs",
    "Debuffs",
    "Cooldowns",
    "Active Spells",
    // resident/popup dialogs that default to the generic renderer
    "bank",
    "befriend",
    "quick",
    "quick-simu",
    "quick-combat",
    "tables",
    "espMasterDialog",
    "mapViewMain",
    "BetrayerPanel",
    "lumnis_day",
    // negatives: per-entity, lich-script, unknown
    "injuries-10154507",
    "UberBar",
    "bugDialogBox",
    "no_such_id",
    "",
];

fn build_golden() -> String {
    let mut out = String::new();

    writeln!(out, "# Template catalog characterization (generated — see file header test)").unwrap();

    // ── 1. Per-game template lists ──────────────────────────────────────
    for (label, game) in [
        ("all", None),
        ("gs4", Some(GameType::GS4)),
        ("dr", Some(GameType::DR)),
    ] {
        writeln!(out, "\n[template_list.{label}]").unwrap();
        for name in Config::list_window_templates_for_game(game) {
            writeln!(out, "{name}").unwrap();
        }
    }

    // ── 2. Game gating per template ─────────────────────────────────────
    writeln!(out, "\n[template_game_type]").unwrap();
    for name in Config::list_window_templates() {
        writeln!(out, "{name} = {:?}", Config::template_game_type(&name)).unwrap();
    }

    // ── 3. The three id-mapping functions over the probe inventory ──────
    writeln!(out, "\n[id_maps]").unwrap();
    for id in PROBE_IDS {
        writeln!(
            out,
            "{:?}: dialog={:?} stream={:?} has_widget={}",
            id,
            Config::dialog_id_to_template(id),
            Config::stream_id_to_template(id),
            Config::id_has_widget_template(id),
        )
        .unwrap();
    }

    // ── 4. Menu content: categories and the addable list on an empty
    //      layout, per game — the "no dead menu" tripwires. Phase 3 swaps
    //      these paths to the resolver+catalog; the snapshots must stay
    //      green UNCHANGED (the transparency proof).
    let categorized = |label: &str,
                       map: std::collections::HashMap<vellum_fe::config::WidgetCategory, Vec<String>>,
                       out: &mut String| {
        writeln!(out, "\n[{label}]").unwrap();
        let mut entries: Vec<_> = map.into_iter().collect();
        entries.sort_by_key(|(category, _)| format!("{category:?}"));
        for (category, names) in entries {
            writeln!(out, "{category:?}: {}", names.join(", ")).unwrap();
        }
    };
    categorized("templates_by_category", Config::get_templates_by_category(), &mut out);
    // Layout has no Default; a windowless TOML is the empty layout.
    let empty_layout: vellum_fe::config::Layout =
        toml::from_str("windows = []").expect("empty layout parses");
    for (label, game) in [
        ("all", None),
        ("gs4", Some(GameType::GS4)),
        ("dr", Some(GameType::DR)),
    ] {
        categorized(
            &format!("addable_on_empty_layout.{label}"),
            Config::get_addable_templates_by_category(&empty_layout, game),
            &mut out,
        );
    }

    // ── 5. Golden TOML of every template's seeded WindowDef ─────────────
    for name in Config::list_window_templates() {
        writeln!(out, "\n[template.{name}]").unwrap();
        match Config::get_window_template(&name) {
            Some(def) => {
                let toml = toml::to_string(&def)
                    .unwrap_or_else(|e| format!("<serialize error: {e}>"));
                for line in toml.lines() {
                    writeln!(out, "  {line}").unwrap();
                }
            }
            None => writeln!(out, "  <listed but get_window_template returned None>")
                .unwrap(),
        }
    }
    out
}

#[test]
fn template_catalog_matches_golden_fixture() {
    let current = build_golden();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_PATH);

    if std::env::var("UPDATE_GOLDEN").is_ok() || !path.exists() {
        std::fs::write(&path, &current).expect("write golden fixture");
        assert!(
            std::env::var("UPDATE_GOLDEN").is_ok(),
            "golden fixture was missing and has been generated at {} — \
             review it, commit it, and re-run",
            path.display()
        );
        return;
    }

    let expected = std::fs::read_to_string(&path).expect("read golden fixture");
    // Normalize line endings so git's CRLF checkout settings can't fail it.
    let normalize = |s: &str| s.replace("\r\n", "\n");
    let (expected, current) = (normalize(&expected), normalize(&current));
    if expected != current {
        // First differing line, for a readable failure.
        let diff_line = expected
            .lines()
            .zip(current.lines())
            .position(|(a, b)| a != b)
            .map(|i| i + 1)
            .unwrap_or_else(|| expected.lines().count().min(current.lines().count()) + 1);
        panic!(
            "template catalog diverged from the Phase 0 golden fixture \
             (first difference at fixture line {diff_line}).\n\
             If this change is DELIBERATE: UPDATE_GOLDEN=1 cargo test \
             --test template_characterization, review the fixture diff, \
             commit it. If not, a template edit leaked."
        );
    }
}

/// The two listing functions must agree: every listed name seeds a def,
/// and the parallel list has no dead or duplicate entries. (The redesign
/// deletes the parallel list; this pins the invariant it must preserve.)
#[test]
fn every_listed_template_seeds_a_window_def() {
    let names = Config::list_window_templates();
    assert!(!names.is_empty());
    let mut seen = std::collections::HashSet::new();
    for name in &names {
        assert!(seen.insert(name.clone()), "duplicate template name {name}");
        assert!(
            Config::get_window_template(name).is_some(),
            "listed template {name} has no definition"
        );
    }
}

/// Unknown ids fall through to the documented defaults — the resolver's
/// kind-fallback layer must reproduce exactly this.
#[test]
fn unknown_ids_fall_through_to_defaults() {
    assert_eq!(Config::dialog_id_to_template("no_such_id"), "no_such_id");
    assert_eq!(Config::stream_id_to_template("no_such_id"), "text_custom");
    assert!(!Config::id_has_widget_template("no_such_id"));
    // Per-entity dialog ids (other players' injury dolls) must NOT be
    // claimed by the exact-id maps.
    assert_eq!(
        Config::dialog_id_to_template("injuries-10154507"),
        "injuries-10154507"
    );
    assert!(!Config::id_has_widget_template("injuries-10154507"));
}
