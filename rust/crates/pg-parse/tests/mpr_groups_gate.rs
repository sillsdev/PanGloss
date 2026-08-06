//! Conformance replay for W3.1 (MPR feature groups) against C#-oracle-generated `rust/conformance/mpr-groups/{required-all,output-overwrite}` fixtures; see each fixture's README for grammar design.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_dir(name: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR = .../rust/crates/pg-parse ; fixtures live at repo_root/rust/conformance.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/mpr-groups")
        .join(name)
}

/// Parses `expected.tsv`'s completed rows into `(word, status, signature)` triples, skipping the interleaved `STARTED` sentinel rows.
fn expected_rows(dir: &Path) -> Vec<(String, String, String)> {
    let text = std::fs::read_to_string(dir.join("expected.tsv")).expect("read expected.tsv");
    text.lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            (cols.len() >= 5).then(|| {
                (
                    cols[1].to_string(),
                    cols[3].to_string(),
                    cols[4].to_string(),
                )
            })
        })
        .collect()
}

fn replay(fixture: &str) {
    let dir = fixture_dir(fixture);
    let xml = std::fs::read_to_string(dir.join("grammar.xml")).expect("read grammar.xml");
    let grammar =
        load(&xml).unwrap_or_else(|e| panic!("mpr-groups/{fixture} grammar failed to load: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    let rows = expected_rows(&dir);
    assert!(
        !rows.is_empty(),
        "mpr-groups/{fixture}/expected.tsv had no completed rows"
    );
    for (word, status, expected_sig) in rows {
        assert_eq!(
            status, "ok",
            "oracle rows in this fixture are all ok-status"
        );
        let got = morpher.parse_word(&word).signature();
        assert_eq!(
            got, expected_sig,
            "mpr-groups/{fixture}: word {word:?} signature mismatch vs C# oracle"
        );
    }
}

/// Self-skip guard: `rust/conformance/` isn't a submodule yet, so `--include-ignored` runs must not panic on the missing directory.
fn have_fixture(name: &str) -> bool {
    fixture_dir(name).join("grammar.xml").exists()
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn mpr_groups_required_all_matches_oracle() {
    if !have_fixture("required-all") {
        eprintln!("skipping: rust/conformance/mpr-groups/required-all not present on disk");
        return;
    }
    replay("required-all");
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn mpr_groups_output_overwrite_matches_oracle() {
    if !have_fixture("output-overwrite") {
        eprintln!("skipping: rust/conformance/mpr-groups/output-overwrite not present on disk");
        return;
    }
    replay("output-overwrite");
}
