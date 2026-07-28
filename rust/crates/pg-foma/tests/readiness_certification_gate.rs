//! Task 5.3 of `openspec/changes/certify-language-readiness` (tasks.md §5; `specs/
//! language-readiness-certification/spec.md`'s "A grammar contains a refused construct" scenario):
//! a test that [`pg_foma::readiness_verdict`]'s `not-supported` tier cites a REAL predicate
//! refusal, on a grammar known to carry a permanently carved-out construct.
//!
//! Per this task's own brief: "all three reference grammars now refuse on exactly
//! `mpr-group.overwrite-output` -- `samples/data/{indonesian,amharic,sena}-hc.xml` are available
//! and any of them is a genuine not-supported case" (`docs/benchmark-matrix.md`'s own finding,
//! reproduced here against the real, current capability gate rather than assumed stale).
//!
//! # Gitignored local corpus data (mirrors `tests/f3_parity.rs` exactly)
//! `samples/data/*` is real-language corpus data, deliberately gitignored (this repo's own
//! synthetic-conformance-only rule: real-language data is never committed). These tests load a
//! real grammar from `samples/data/`, so -- exactly like every `samples/data`-dependent test in
//! `tests/f3_parity.rs` -- they are unconditionally `#[ignore]`d with a self-skip guard, so a
//! `--include-ignored` run stays green in an environment without the fixture (CI, a fresh clone).
//! Run locally with `cargo test -p pg-foma --test readiness_certification_gate -- --include-ignored`.

use std::path::{Path, PathBuf};

use pg_foma::readiness_policy::policy_v1;
use pg_foma::readiness_verdict::{certify, CapabilitySummary, CheckOutcome, Tier, TrustStatus};
use pg_grammar::model::Grammar;

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

/// Self-skip guard: gitignored real-corpus fixtures aren't present in a fresh clone or CI.
fn have(name: &str) -> bool {
    sample_path(name).exists()
}

fn load_grammar(xml_name: &str) -> Grammar {
    let path = sample_path(xml_name);
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {xml_name}: {e}"))
}

/// Runs the not-supported-cites-a-real-refusal assertion against one reference grammar.
fn assert_not_supported_names_overwrite_output(xml_name: &str) {
    let g = load_grammar(xml_name);
    let policy = policy_v1();

    // No compiled artifact/measurements at all -- this grammar is refused before anything would
    // compile, matching `Measurements`'s own documented `None` case.
    let report = certify(&g, &TrustStatus::Proven, None, &policy);

    assert_eq!(
        report.tier,
        Tier::NotSupported,
        "{xml_name}: expected the not-supported tier for a permanently-refused grammar, got {:?}",
        report.tier
    );

    let refusals = match &report.capability {
        CapabilitySummary::Refuse { refusals } => refusals,
        other => panic!(
            "{xml_name}: expected CapabilitySummary::Refuse (per docs/benchmark-matrix.md's own \
             finding that every reference grammar carries the permanent mpr-group.overwrite-output \
             carve-out), got {other:?} -- if this construct's disposition has genuinely changed, \
             pick a different verified-refused fixture"
        ),
    };
    assert!(
        !refusals.is_empty(),
        "{xml_name}: the not-supported tier must cite at least one real refusal"
    );
    assert!(
        refusals
            .iter()
            .any(|r| r.predicate == "mpr-group.overwrite-output"),
        "{xml_name}: expected mpr-group.overwrite-output among the real refusals, got {refusals:?}"
    );
    // Every cited refusal must actually name both a predicate and a construct -- an empty
    // construct string would be a citation in name only.
    for r in refusals {
        assert!(
            !r.predicate.is_empty(),
            "{xml_name}: refusal must name a predicate: {r:?}"
        );
        assert!(
            !r.construct.is_empty(),
            "{xml_name}: refusal must name a construct: {r:?}"
        );
    }

    // Every check must have been forced to NotAssessed (no compiled artifact exists to measure),
    // never silently rendered as passed -- a not-supported grammar has no artifact to measure at
    // all, and the report must say so plainly rather than presenting a stale/guessed threshold
    // result.
    assert!(
        report
            .checks
            .iter()
            .all(|c| matches!(c.outcome, CheckOutcome::NotAssessed { .. })),
        "{xml_name}: every check must be NotAssessed with no compiled artifact: {:?}",
        report.checks
    );
    assert!(
        !report.is_certified(),
        "{xml_name}: a not-supported grammar must never certify"
    );

    // The report's own notes must explain the not-supported tier in terms of the real capability
    // evaluation -- not merely say "not passing" (design.md: "a bare 'not passing' is useless").
    assert!(
        report.notes.iter().any(|n| n.contains("NOT SUPPORTED")),
        "{xml_name}: report notes must explain the not-supported tier: {:?}",
        report.notes
    );
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with \
            --include-ignored"]
fn indonesian_reference_grammar_is_not_supported_citing_overwrite_output() {
    if !have("indonesian-hc.xml") {
        eprintln!("skip: samples/data/indonesian-hc.xml not present locally");
        return;
    }
    assert_not_supported_names_overwrite_output("indonesian-hc.xml");
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with \
            --include-ignored"]
fn amharic_reference_grammar_is_not_supported_citing_overwrite_output() {
    if !have("amharic-hc.xml") {
        eprintln!("skip: samples/data/amharic-hc.xml not present locally");
        return;
    }
    assert_not_supported_names_overwrite_output("amharic-hc.xml");
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with \
            --include-ignored"]
fn sena_reference_grammar_is_not_supported_citing_overwrite_output() {
    if !have("sena-hc.xml") {
        eprintln!("skip: samples/data/sena-hc.xml not present locally");
        return;
    }
    assert_not_supported_names_overwrite_output("sena-hc.xml");
}
