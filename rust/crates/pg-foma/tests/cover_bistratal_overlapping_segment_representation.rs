//! `conformance-staging/edge-cases/bistratal-overlapping-segment-representation`'s own regression
//! gate (docs/conformance/representative-typology-basis.md S1.2.5): pins the CURRENT, honest
//! behavior for a two-table grammar whose tables share a literal representation ("s") denoting a
//! DIFFERENT segment identity in each --
//!
//! 1. the capability gate's `Refuse` verdict (`multi-table.faithful-table-threading`), and
//! 2. the oracle's (`pg_parse::Morpher`) own correct, unaffected analysis of every reachable word.
//!
//! This test is the one that should FAIL -- prompting deliberate review -- the day
//! `MultiTableFaithfulThreadingPredicate` is promoted to admit a shared-representation
//! configuration (e.g. via the PUA-reserved-range-per-table encoding design.md's own doc proposes).

use std::fs;
use std::path::Path;

use pg_foma::capability::CompileDecision;
use pg_foma::capability_entry::evaluate_capability;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../conformance-staging/edge-cases/bistratal-overlapping-segment-representation/grammar.xml",
    )
}

fn load() -> Grammar {
    let path = fixture_path();
    let xml = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// The capability gate's own verdict: `Refuse`, naming `MultiTable`, for the two tables' shared
/// "s" representation.
#[test]
fn capability_gate_refuses_shared_representation_across_tables() {
    let g = load();
    assert_eq!(g.char_tables.len(), 2, "fixture must declare exactly 2 tables");
    match evaluate_capability(&g) {
        CompileDecision::Refuse(diags) => {
            assert!(
                diags.iter().any(|d| d.predicate == "multi-table.faithful-table-threading"),
                "expected the multi-table.faithful-table-threading predicate to refuse: {diags:?}"
            );
        }
        other => panic!(
            "expected Refuse for two tables sharing a representation, got {other:?}"
        ),
    }
}

/// The Outer stratum's own roots (table t2, the grammar's LAST/surface table) parse correctly via
/// the oracle despite the shared "s" spelling -- `pg_parse::Morpher` resolves each table's own
/// segment identity explicitly, so the shared-representation ambiguity that afflicts the FST's
/// raw-per-table-index token scheme never reaches this codebase's own oracle.
#[test]
fn outer_stratum_roots_parse_correctly_despite_shared_representation() {
    let g = load();
    let morpher = Morpher::new(&g, usize::MAX);

    for (word, expected) in [("des", "ROOT3|des"), ("sed", "ROOT4|sed")] {
        let outcome = morpher.parse_word_opts(word, &ParseOptions::default());
        assert!(!outcome.invalid_shape, "{word:?} must not be SKIPPED");
        assert_eq!(outcome.signature(), expected, "{word:?} signature mismatch");
    }

    // Negative control: "eds" is well-formed over table t2's own alphabet but names no real entry.
    let outcome = morpher.parse_word_opts("eds", &ParseOptions::default());
    assert!(!outcome.invalid_shape);
    assert_eq!(outcome.analyses.len(), 0);
}

/// The Inner stratum's own roots (table t1, non-final) are SKIPPED (invalid shape) by the oracle --
/// a separate, honestly-documented architectural fact (this codebase's surface-tokenization
/// convention uses only the grammar's LAST stratum's table), not itself a MultiTable-specific
/// finding, but worth pinning explicitly rather than leaving unexercised.
#[test]
fn inner_stratum_roots_are_unreachable_at_the_surface() {
    let g = load();
    let morpher = Morpher::new(&g, usize::MAX);

    for word in ["basi", "abis"] {
        let outcome = morpher.parse_word_opts(word, &ParseOptions::default());
        assert!(
            outcome.invalid_shape,
            "{word:?} (an Inner-stratum-only root) was expected to be SKIPPED (invalid shape) -- \
             if this now segments, the surface-tokenization convention changed; review before \
             updating this assertion"
        );
    }
}
