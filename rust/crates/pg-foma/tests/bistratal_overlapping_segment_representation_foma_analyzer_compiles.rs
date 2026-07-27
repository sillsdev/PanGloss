//! Task #45 regression: `FomaAnalyzer::new` used to PANIC (`index out of bounds: the len is 3 but
//! the index is 3`) on `conformance-staging/edge-cases/bistratal-overlapping-segment-representation`
//! -- a crash, never an acceptable outcome, on a construct this crate's own capability gate already
//! grades `ConfirmOnly` (`tests/cover_bistratal_overlapping_segment_representation.rs`'s own
//! `capability_gate_confirm_only_for_shared_representation_across_tables`).
//!
//! ## Root cause
//! `pg_foma::emit::collect_roots` (called from `emit_with_budget_profiled`, the production
//! `SurfaceProbed` path `FomaAnalyzer`/`FomaProposer` use) used to take ONE `table: &CharDefTable`
//! argument for the whole grammar -- always `emit::surface_table(g)`, i.e. the grammar's LAST
//! stratum's own char-def table -- and indexed EVERY root allomorph's `Shape` char-def ids against
//! it, regardless of which stratum (and therefore which table) that allomorph actually belongs to.
//! This fixture declares two strata over two DIFFERENT, differently-sized tables ("Inner"/`t1`, 4
//! segments incl. `cs1` at index 3; "Outer"/`t2`, only 3 segments, max valid index 2) -- so an
//! Inner-stratum root allomorph's char-def id 3 (`cs1`, its "s") got looked up against `t2`
//! (len 3), a genuine out-of-bounds `Vec` index, panicking inside `CharDefTable::get` rather than
//! refusing or simply resolving the correct table. This is the same "implicit table-zero/table-N
//! default" antipattern class `pg_foma::replace::owning_table` already fixes for rewrite rules
//! (per-rule, resolved via its own stratum) -- root-allomorph collection was the one path that never
//! got the analogous fix. `collect_roots` now resolves each entry's table fresh, per stratum
//! (`g.char_tables[sd.table.0 as usize]`, the same field `pg_parse::Morpher` already resolves
//! per-rule via an explicit `TableId`), so an allomorph is always indexed against the table it was
//! actually parsed against.
//!
//! ## Correct behavior
//! Not a decline: this fixture's own capability verdict is `ConfirmOnly` (this crate's regression
//! test above), and a construct this crate already knows how to grade `ConfirmOnly` compiles for
//! real once the table-blindness bug is fixed -- `FomaAnalyzer::new` now returns `Ok(_)`, never
//! panicking, and the Outer-stratum (`t2`, the grammar's surface/last table) roots it lexicalizes
//! remain analyzable exactly as `tests/cover_bistratal_overlapping_segment_representation.rs`'s own
//! `outer_stratum_roots_parse_correctly_despite_shared_representation` already pins for the oracle.

use std::fs;
use std::path::Path;

use pg_foma::composite::FomaAnalyzer;
use pg_grammar::model::Grammar;

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../conformance-staging/edge-cases/bistratal-overlapping-segment-representation/grammar.xml",
    )
}

fn load() -> Grammar {
    let path = fixture_path();
    let xml = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// The headline regression: `FomaAnalyzer::new` must not panic, and (this fixture's capability
/// verdict being `ConfirmOnly`, never `Refuse`) must actually compile.
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

/// The table-blindness fix must not merely avoid the crash -- the Outer-stratum ("t2", this
/// grammar's surface/last table) roots must still analyze correctly through the compiled network,
/// same ground truth `tests/cover_bistratal_overlapping_segment_representation.rs`'s own oracle-side
/// test already pins.
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
