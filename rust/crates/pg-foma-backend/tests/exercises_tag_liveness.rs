//! Every fixture `exercises:` tag must exactly match a literal `machine/conformance/constructs.txt` row id; an unmatched tag would otherwise silently contribute zero coverage instead of failing loudly.

use std::collections::BTreeSet;
use std::path::PathBuf;

use pg_conformance_fixtures::discover;

/// Anchored at `CARGO_MANIFEST_DIR`, never the process CWD, which differs between `cargo test` and a bare test-binary invocation.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Parses `constructs.txt`'s row ids (one per line, `#`-comments and blank lines ignored); a line parser only, not a second fixture-discovery implementation.
fn known_construct_ids() -> BTreeSet<String> {
    let path = repo_root().join("machine/conformance/constructs.txt");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Every `exercises:` tag, on every word and parse, across both discovery roots, must be a known `constructs.txt` row id.
#[test]
fn every_exercises_tag_is_a_known_construct_id() {
    let known = known_construct_ids();
    assert!(
        !known.is_empty(),
        "constructs.txt parsed to zero rows -- the file moved, or the blank-line/`#`-comment \
         parsing rule broke; this gate cannot be trusted to check anything in that state"
    );

    let mut checked = 0usize;
    let mut offenders: Vec<String> = Vec::new();

    for f in discover() {
        let words_yaml = f.load_words_yaml();
        for w in &words_yaml.words {
            for tag in &w.exercises {
                checked += 1;
                if !known.contains(tag) {
                    offenders.push(format!(
                        "{} word {:?}: exercises tag {:?} is not a constructs.txt row id",
                        f.label(),
                        w.word,
                        tag
                    ));
                }
            }
            for p in &w.parses {
                for tag in &p.exercises {
                    checked += 1;
                    if !known.contains(tag) {
                        offenders.push(format!(
                            "{} word {:?} parse (signature {:?}): exercises tag {:?} is not a \
                             constructs.txt row id",
                            f.label(),
                            w.word,
                            p.signature,
                            tag
                        ));
                    }
                }
            }
        }
    }

    assert!(
        checked > 0,
        "scanned zero exercises: tags across both discovery roots -- the words.yaml schema or \
         field name changed and this gate went vacuous, which is worse than a failure (it would \
         silently stop protecting the coverage cross-check); see this file's own top-doc for why"
    );
    assert!(
        offenders.is_empty(),
        "{} exercises: tag(s) do not match any machine/conformance/constructs.txt row id \
         (byte-for-byte, per constructs.txt's own header comment). An unrecognized tag silently \
         contributes ZERO coverage in conformance_coverage::construct_ids_for's cross-check -- \
         exactly how LeftToRightRewrite/SubruleGating/RightToLeftRewrite/MultiTable sat Uncovered \
         while their fixtures looked correct. Fix the tag to the exact constructs.txt row id it \
         should have been:\n  {}",
        offenders.len(),
        offenders.join("\n  ")
    );
}
