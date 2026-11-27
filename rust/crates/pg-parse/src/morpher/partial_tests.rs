use super::*;

#[test]
fn surface_mismatch_records_the_actual_surface_gate_without_extra_search_work() {
    let fixture =
        pg_conformance_fixtures::require_fixture("edge-cases", "guesser-pattern-root-fallback");
    let g = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    let m = Morpher::new(&g, usize::MAX);
    let shape = segment_with_features(&g, &g.char_tables[0], "kad").unwrap();
    let mut word = Word::new(shape, StratumId(0));
    let mut work = Vec::new();
    for limit in [0, 3] {
        word.set_collect_partials(limit != 0);
        let budget = pg_fst::work::WorkBudget::new(100);
        let _budget = budget.enter();
        let scope = pg_rules::partials::enter(limit);
        if limit != 0 {
            m.prepare_partial_seed(&word, &word.trail);
        }
        assert!(!m.is_match_traced(&word, "gag", &NoopSink, TraceHandle::DUMMY, None));
        work.push(budget.used());
        let partials = scope.finish();
        if limit != 0 {
            assert_eq!(partials.len(), 1);
            let p = crate::PartialAnalysis::from_rejection(partials.into_iter().next().unwrap());
            assert_eq!(p.failure_kind, "surface-mismatch");
            assert_eq!(p.rejecting_object_id, "stratum:0");
            assert_eq!(p.residue, "kad");
        } else {
            assert!(partials.is_empty());
        }
    }
    assert_eq!(work[0], work[1]);
}
