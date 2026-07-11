//! Conformance replay for W3.1 (MPR feature groups): `rust/conformance/mpr-groups/{required-all,
//! output-overwrite}`. Both `expected.tsv` files are C#-oracle-generated (parse-opt @ `ccf750e6`);
//! see each fixture's README for the grammar design and the distinguishing rows.
//!
//! Red-on-revert:
//! - `required-all`: revert `Grammar::mpr_group_ok` (back to flat `have.overlaps(required)`) and
//!   `sodz` starts parsing (`+|sodz` vs the oracle's `-`) — the All-group's second member is no
//!   longer demanded.
//! - `output-overwrite`: revert `Grammar::mpr_add_output` (back to plain union) and `yxpitz`
//!   starts parsing — ruleY's Overwrite no longer drops the group-sibling `mprA` that ruleZ
//!   requires.

use std::path::{Path, PathBuf};

use hc_grammar::load;
use hc_parse::Morpher;

fn fixture_dir(name: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR = .../rust/crates/hc-parse ; fixtures live at repo_root/rust/conformance.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/mpr-groups")
        .join(name)
}

/// Parse `expected.tsv`'s completed rows (`idx \t word \t ms \t status \t signature`) into
/// `(word, status, signature)` triples, skipping the interleaved `STARTED` sentinel rows.
fn expected_rows(dir: &Path) -> Vec<(String, String, String)> {
    let text = std::fs::read_to_string(dir.join("expected.tsv")).expect("read expected.tsv");
    text.lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            (cols.len() >= 5).then(|| (cols[1].to_string(), cols[3].to_string(), cols[4].to_string()))
        })
        .collect()
}

fn replay(fixture: &str) {
    let dir = fixture_dir(fixture);
    let xml = std::fs::read_to_string(dir.join("grammar.xml")).expect("read grammar.xml");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("mpr-groups/{fixture} grammar failed to load: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    let rows = expected_rows(&dir);
    assert!(!rows.is_empty(), "mpr-groups/{fixture}/expected.tsv had no completed rows");
    for (word, status, expected_sig) in rows {
        assert_eq!(status, "ok", "oracle rows in this fixture are all ok-status");
        let got = morpher.parse_word(&word).signature();
        assert_eq!(
            got, expected_sig,
            "mpr-groups/{fixture}: word {word:?} signature mismatch vs C# oracle"
        );
    }
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn mpr_groups_required_all_matches_oracle() {
    replay("required-all");
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn mpr_groups_output_overwrite_matches_oracle() {
    replay("output-overwrite");
}
