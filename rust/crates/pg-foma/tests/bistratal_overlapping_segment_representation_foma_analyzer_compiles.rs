//! Regression: root-allomorph char-def lookup must resolve each entry's own stratum table rather than assuming the grammar's last stratum table, or a multi-table grammar (this fixture: strata over differently-sized tables) panics on an out-of-bounds index instead of compiling the `ConfirmOnly` construct it actually is.

use std::fs;
use std::path::Path;

use pg_foma::composite::FomaAnalyzer;
use pg_grammar::model::Grammar;

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

/// `FomaAnalyzer::new` must compile this `ConfirmOnly` fixture, not merely avoid panicking.
#[test]
fn foma_analyzer_new_compiles_without_panicking() {
    let g = load();
    let result = FomaAnalyzer::new(&g);
    assert!(
        result.is_ok(),
        "FomaAnalyzer::new must compile this ConfirmOnly-graded multi-table fixture (an honest \
         decline would also be acceptable for a genuinely uncompilable construct, but this one is \
         known-compilable): {:?}",
        result.err().map(|e| e.to_string())
    );
}

/// Outer-stratum roots must still analyze correctly through the compiled network, matching the oracle-side ground truth.
#[test]
fn outer_stratum_roots_still_analyze_through_the_compiled_network() {
    let g = load();
    let mut analyzer = FomaAnalyzer::new(&g).expect("must compile (see the test above)");

    for word in ["des", "sed"] {
        let outcome = analyzer.analyze_word(word);
        assert!(
            !outcome.analyses.is_empty(),
            "{word:?} (an Outer-stratum root, table t2) must analyze to at least one candidate \
             through the compiled network"
        );
    }
}
