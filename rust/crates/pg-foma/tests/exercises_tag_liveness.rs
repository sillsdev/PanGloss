//! Systemic gate: every fixture `exercises:` tag must be a literal `machine/conformance/
//! constructs.txt` row id.
//!
//! # Why this gate exists
//! `conformance_coverage::construct_ids_for` maps each `pg_foma::capability::CharacteristicKind`
//! to `constructs.txt` row-id strings, and `passing_covered_constructs`
//! (`tests/conformance_coverage_gate.rs`) credits a construct toward coverage by collecting every
//! `words.yaml` `exercises:` string from a currently-passing word/parse and set-matching it
//! against those ids, **byte-for-byte**. Before this gate existed, three staged fixtures wrote the
//! *characteristic name* instead of the *row id* --
//! `machine/conformance/edge-cases/subrule-morphosyntactic-gating` tagged `"SubruleGating"`
//! (the id is `"RewriteSubruleDef gating: required/excluded POS or MPR at the subrule level"`,
//! `constructs.txt` row 31), `right-to-left-bounded-quantifier-rewrite` tagged
//! `"RightToLeftRewrite"` (the id is `"RewriteRule direction (Dir): right-to-left"`, row 30), and
//! `bistratal-overlapping-segment-representation` tagged `"MultiTable"` (the id is
//! `"CharacterDefinitionTable: more than one table, one per stratum"`, row 36). Because none of
//! those strings matched any row id, **those tags contributed exactly zero coverage** -- silently.
//! The fixtures looked correct (right grammar, right signature, a plausible-looking `exercises:`
//! entry); nothing failed; the corresponding `constructs.txt` rows simply sat `Uncovered` in the
//! cross-check forever. That is a strictly worse failure mode than a loud error: a typo/rename
//! that used to be a soft, easy-to-miss warning (per `constructs.txt`'s own header: "The harness
//! treats an `exercises:` value that ISN'T in this file as a soft warning... never a hard error")
//! is exactly how all three rows -- plus a fourth, `LeftToRightRewrite`, which had no tag at all --
//! sat unnoticed. See each corrected fixture's own `STAGING.md` "Coverage-tag correction" section
//! for the concrete history this gate exists to stop recurring. This file's reasoning mirrors
//! `tests/coverage_citation_liveness.rs` (read first if this doc feels familiar): a string that
//! silently resolves to nothing is worse than a failing assertion, because nothing ever calls
//! attention to it.
//!
//! # What it checks
//! Every `exercises:` entry -- both per-word (`WordEntry::exercises`) and per-parse
//! (`ParseEntry::exercises`) -- across every fixture returned by
//! `pg_conformance_fixtures::discover` (the ONE shared enumeration helper this repo's fixture
//! tests use for both `machine/conformance/**` and `conformance-staging/**`; this file does not
//! walk either root a second time) names a literal, verbatim line of `machine/conformance/
//! constructs.txt` (blank lines and `#`-led comment lines excluded, per that file's own header).
//!
//! # What it deliberately does NOT check
//! Whether a fixture's `exercises:` tag is the RIGHT tag for what that word/parse actually does --
//! i.e. that the fixture doesn't *overclaim* a construct it doesn't genuinely exercise. That is a
//! human-authoring discipline (the `conformance-grammars` skill's own "never tag a construct a
//! fixture does not exercise" rule), not something a string-membership check can discharge. This
//! gate closes the purely mechanical half: the tag string cannot be a dangling reference to a row
//! that does not exist.

use std::collections::BTreeSet;
use std::path::PathBuf;

use pg_conformance_fixtures::discover;

/// Repo root, from this crate's own `CARGO_MANIFEST_DIR` (`rust/crates/pg-foma`) -- never a path
/// relative to the process CWD, which differs between `cargo test` and a bare test-binary
/// invocation.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Parse `machine/conformance/constructs.txt` into its set of row ids. Format per the file's own
/// header comment: "one construct per line. Blank lines and lines starting with `#` are ignored."
/// This is a single-file line parser, not a second fixture-discovery/path-walking implementation
/// (that concern stays entirely inside `pg_conformance_fixtures::discover`).
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

/// The gate itself: every `exercises:` tag, on every word and every parse, across every fixture
/// under both discovery roots, must be a known `constructs.txt` row id.
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
