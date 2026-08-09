//! Pins the current behavior for a two-table grammar whose tables share a literal representation ("s") denoting a different segment identity in each: the capability gate's `ConfirmOnly` verdict (a shared representation is a false-negative risk, not false-positive, so `Refuse` would be over-conservative), and the oracle's own unaffected analysis of every reachable word. This fixture has no rule threading material between the two tables — `tests/two_table_shared_representation_recall.rs` is the one that exercises cross-table aliasing firing.

use std::fs;
use std::path::Path;

use pg_foma::capability::CompileDecision;
use pg_foma::capability_entry::best_case_across_backends_for_grammar;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../machine/conformance/edge-cases/bistratal-overlapping-segment-representation/grammar.xml",
    )
}

fn load() -> Grammar {
    let path = fixture_path();
    let xml = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// The capability gate's own verdict: `ConfirmOnly` (never `Refuse`), for the two tables' shared "s" representation.
#[test]
fn capability_gate_confirm_only_for_shared_representation_across_tables() {
    let g = load();
    assert_eq!(
        g.char_tables.len(),
        2,
        "fixture must declare exactly 2 tables"
    );
    assert_eq!(
        best_case_across_backends_for_grammar(&g),
        CompileDecision::ConfirmOnly,
        "two tables sharing a representation must ConfirmOnly, never Refuse or Admit, after the \
         cross-table aliasing fix"
    );
}

/// The Outer stratum's roots (table t2, the surface table) parse correctly via the oracle despite the shared "s" spelling: `Morpher` resolves each table's segment identity explicitly, unlike the FST's per-table-index token scheme.
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

/// The Inner stratum's roots (table t1, non-final) are skipped (invalid shape) by the oracle: surface tokenization uses only the grammar's last stratum's table, a fact worth pinning explicitly.
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
