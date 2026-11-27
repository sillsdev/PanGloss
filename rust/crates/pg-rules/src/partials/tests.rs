use super::*;

#[test]
fn ranking_is_shortest_unicode_residue_then_earliest_work_and_bounds_storage() {
    for limit in 1..=3 {
        let scope = enter(limit);
        let budget = pg_fst::work::WorkBudget::new(100);
        let _work = budget.enter();
        for (n, residue) in ["long", "é", "z", "x", "verylong"].into_iter().enumerate() {
            assert!(budget.consume());
            set_seed(Seed::new(vec![format!("m{n}")], residue.into()));
            record(FailureKind::NoRoot, RejectingObject::Stratum(StratumId(0)));
            STATE.with(|state| assert!(state.borrow().best.len() <= limit as usize));
        }
        let best = scope.finish();
        assert_eq!(best.len(), limit as usize);
        let expected = ["é", "z", "x"];
        for (item, expected) in best.iter().zip(expected) {
            assert_eq!(item.seed.residue, expected);
        }
        assert_eq!(budget.used(), 5, "collection consumes no search work");
    }
}

#[test]
fn repeated_gate_does_not_fill_the_list_and_scopes_restore_on_unwind() {
    let scope = enter(3);
    set_seed(Seed::new(vec![], "a".into()));
    for _ in 0..1000 {
        record(FailureKind::RootGate, RejectingObject::Root(LexEntryId(0)));
    }
    assert!(std::panic::catch_unwind(|| {
        let _inner = enter(0);
        record(FailureKind::NoRoot, RejectingObject::Stratum(StratumId(0)));
        panic!("exercise scope cleanup");
    })
    .is_err());
    assert_eq!(scope.finish().len(), 1);
    assert!(enter(0).finish().is_empty());
}

#[test]
fn eviction_defers_last_seed_drop_until_the_next_candidate() {
    let scope = enter(3);
    let mut old = Vec::new();
    for residue in ["verylong", "longer", "long"] {
        set_seed(Seed::new(vec!["m".repeat(1000); 100], residue.into()));
        STATE.with(|state| old.push(Rc::downgrade(state.borrow().seed.as_ref().unwrap())));
        record(FailureKind::NoRoot, RejectingObject::Stratum(StratumId(0)));
    }
    set_seed(Seed::new(vec![], "x".into()));
    for i in 0..100 {
        record(FailureKind::AffixGate, RejectingObject::Rule(MRuleId(i)));
        STATE.with(|state| assert!(state.borrow().deferred_len <= 3));
    }
    assert!(old.iter().all(|seed| seed.upgrade().is_some()));
    STATE.with(|state| assert_eq!(state.borrow().deferred_len, 3));
    set_seed(Seed::new(vec![], "y".into()));
    assert!(old.iter().all(|seed| seed.upgrade().is_none()));
    assert_eq!(scope.finish().len(), 3);
}

#[test]
fn disabled_validity_rejection_never_borrows_the_collector() {
    let fixture =
        pg_conformance_fixtures::require_fixture("edge-cases", "guesser-pattern-root-fallback");
    let mut grammar = fixture.load_grammar();
    grammar.entries[0].allomorphs[0].is_bound = true;
    let word =
        crate::morph::seed_from_entry(&grammar, LexEntryId(0), pg_featstruct::FeatureStruct::EMPTY);
    assert!(!word.collect_partials());
    STATE.with(|state| {
        let _exclusive = state.borrow_mut();
        assert!(!crate::validity::allomorphs_valid(&grammar, &word));
    });
}
