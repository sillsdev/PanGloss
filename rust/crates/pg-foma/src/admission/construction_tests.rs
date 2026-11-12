use super::tests::{load, prefix, rule, spec, suffix, CTX_ANY};
use super::tiled_tests::{no_guess, setup, Setup};
use super::*;

fn affixes() -> Grammar {
    let rules = vec![prefix(0, "ka"), suffix(1, "ta"), suffix(2, "nu")];
    load(&spec("kaqtnu", rules, &["qt", "tq"]))
}

const WORDS: [&str; 6] = ["qt", "kaqt", "qtnu", "kaqtta", "qq", "tqta"];

fn refused(result: Result<RuleAdmission, AdmissionRefusal>) -> (u64, u64) {
    match result {
        Err(AdmissionRefusal::ConstructionBudget { spent, budget }) => (spent, budget),
        Err(other) => panic!("another refusal: {other:?}"),
        Ok(_) => panic!("built within the budget"),
    }
}

#[test]
fn the_production_build_does_not_serialize_the_net() {
    let g = affixes();
    let plain = build_rule_admission(&FomaOptions::default(), &g).expect("builds");
    assert_eq!(plain.evidence().final_bytes, None);
    let sized = build_rule_admission_sized(&FomaOptions::default(), &g).expect("builds");
    assert!(sized.evidence().final_bytes.is_some_and(|bytes| bytes > 0));
    for word in WORDS {
        assert_eq!(plain.admit(word), sized.admit(word), "{word}");
    }
}

#[test]
fn construction_units_are_the_same_on_every_run_and_thread() {
    let g = affixes();
    let units = |g: &Grammar| {
        let admission = build_rule_admission(&FomaOptions::default(), g).expect("builds");
        let tiled = build_tiled(&FomaOptions::default(), g, &admission);
        (admission.construction_units(), tiled.construction_units())
    };
    let first = units(&g);
    assert!(first.0 > 0 && first.1 > 0, "{first:?}");
    assert_eq!(first, units(&g));
    let threads: Vec<(u64, u64)> = std::thread::scope(|scope| {
        let runs: Vec<_> = (0..3).map(|_| scope.spawn(|| units(&g))).collect();
        runs.into_iter()
            .map(|run| run.join().expect("run"))
            .collect()
    });
    assert!(threads.iter().all(|seen| *seen == first), "{threads:?}");
}

#[test]
fn an_admission_build_stops_at_its_budget_and_an_unlimited_one_is_unchanged() {
    let g = affixes();
    let opts = FomaOptions::default();
    let full = build_rule_admission(&opts, &g).expect("builds");
    let cost = full.construction_units();
    let (spent, budget) = refused(build_rule_admission_within(&opts, &g, 0));
    assert_eq!(budget, 0);
    assert!(spent <= cost);
    let (spent, budget) = refused(build_rule_admission_within(&opts, &g, cost - 1));
    assert_eq!(budget, cost - 1);
    assert!(spent <= cost);
    let exact = build_rule_admission_within(&opts, &g, cost).expect("the exact cost suffices");
    assert_eq!(exact.construction_units(), cost);
    let unlimited = build_rule_admission_within(&opts, &g, UNLIMITED_UNITS).expect("builds");
    for word in WORDS {
        assert_eq!(full.admit(word), exact.admit(word), "{word}");
        assert_eq!(full.admit(word), unlimited.admit(word), "{word}");
    }
    assert_eq!(full.free(), exact.free());
    let reason = AdmissionRefusal::ConstructionBudget {
        spent: 7,
        budget: 3,
    }
    .reason();
    assert_eq!(reason, "construction-budget:7 of 3 units");
}

#[test]
fn a_tiled_build_stops_at_its_budget_and_an_unlimited_one_is_unchanged() {
    let g = affixes();
    let opts = FomaOptions::default();
    let Setup { admission, tiled } = setup(&g);
    let cost = tiled.construction_units();
    assert!(tiled.active() && cost > 0);
    for budget in [0, cost - 1] {
        let short = build_tiled_within(&opts, &g, &admission, budget);
        assert!(!short.active());
        assert!(short.evidence().budget_exhausted);
        assert!(short.construction_units() <= cost);
        let reason = short.inactive_reason().expect("a reason");
        assert!(reason.contains("construction-budget"), "{reason}");
        assert!(reason.contains(&format!("of {budget} units")), "{reason}");
        assert!(short.accepts("qq"));
    }
    let exact = build_tiled_within(&opts, &g, &admission, cost);
    assert!(exact.active(), "{:?}", exact.inactive_reason());
    assert!(!exact.evidence().budget_exhausted);
    assert_eq!(exact.construction_units(), cost);
    for word in WORDS {
        let admitted = admission.admit(word);
        let (a, b) = (
            tiled.rejects(word, &admitted, &no_guess()),
            exact.rejects(word, &admitted, &no_guess()),
        );
        assert_eq!(a, b, "{word}");
    }
}

#[test]
fn the_prospect_answers_before_any_net_is_built() {
    let g = affixes();
    let Setup { admission, tiled } = setup(&g);
    let prospect = tiled_prospect(&g, &admission);
    assert_eq!(prospect.cannot_be_active(), None);
    for word in WORDS {
        let admitted = admission.admit(word);
        for options in [no_guess(), no_guess().with_guess_root(true)] {
            let (early, built) = (
                prospect.applies(&admitted, &options),
                tiled.applies(&admitted, &options),
            );
            assert_eq!(early, built, "{word}");
        }
    }
    assert!(!prospect.applies(&[], &no_guess()));
    let from = build_tiled_from(&FomaOptions::default(), &g, prospect, UNLIMITED_UNITS);
    assert_eq!(from.active(), tiled.active());
    assert_eq!(from.construction_units(), tiled.construction_units());
}

#[test]
fn the_prospect_names_the_causes_that_need_no_net() {
    let modify = rule(
        1,
        &[CTX_ANY],
        &[r#"<ModifyFromInput index="p0"><SimpleContext naturalClass="ncAny" /></ModifyFromInput>"#.to_owned()],
    );
    let g = load(&spec("kaqt", vec![prefix(0, "ka"), modify], &["qt"]));
    let Setup { admission, tiled } = setup(&g);
    let prospect = tiled_prospect(&g, &admission);
    let reason = prospect.cannot_be_active().expect("a reason");
    assert!(reason.contains("admitted for every word"), "{reason}");
    assert_eq!(tiled.inactive_reason(), Some(reason));
    assert_eq!(tiled.construction_units(), 0);
    let other = affixes();
    let foreign = tiled_prospect(&other, &admission);
    let reason = foreign.cannot_be_active().expect("a reason");
    assert!(reason.contains("another grammar"), "{reason}");
}

#[test]
fn counted_walks_return_the_plain_answers_and_count_the_transitions() {
    let g = affixes();
    let Setup { admission, tiled } = setup(&g);
    assert_eq!(admission.admit_counted("", &mut 0), admission.admit(""));
    let mut total = 0;
    for word in WORDS {
        let mut steps = 0;
        let admitted = admission.admit_counted(word, &mut steps);
        assert_eq!(admitted, admission.admit(word), "{word}");
        let tables = admission.evidence().tables.rules as u64;
        let length = word.chars().count() as u64;
        assert!(steps > 0 && steps <= tables * length, "{word}: {steps}");
        total += steps;
        let mut walked = 0;
        let rejected = tiled.rejects_counted(word, &admitted, &no_guess(), &mut walked);
        assert_eq!(
            rejected,
            tiled.rejects(word, &admitted, &no_guess()),
            "{word}"
        );
        assert!(walked > 0 && walked <= length, "{word}: {walked}");
        let mut again = 0;
        assert_eq!(tiled.accepts_counted(word, &mut again), tiled.accepts(word));
        assert_eq!(again, walked);
    }
    assert!(total > 0);
    let guessing = no_guess().with_guess_root(true);
    let admitted = admission.admit("qq");
    let mut skipped = 0;
    assert!(!tiled.rejects_counted("qq", &admitted, &guessing, &mut skipped));
    assert_eq!(skipped, 0);
}

#[test]
fn nothing_survives_a_refused_build_into_the_next_unlimited_one() {
    let g = affixes();
    let opts = FomaOptions::default();
    let fresh = build_rule_admission(&opts, &g).expect("builds");
    let fresh_tiled = build_tiled(&opts, &g, &fresh);
    let cost = fresh.construction_units();
    let tiled_cost = fresh_tiled.construction_units();
    let texts = [
        "kaqt", "qtnu", "kaqtta", "qq", "tqta", "kaqtnu", "ta", "", "qtqt",
    ];
    for budget in [0, 1, cost / 8, cost / 2, cost - 1] {
        refused(build_rule_admission_within(&opts, &g, budget));
        let again = build_rule_admission(&opts, &g).expect("builds");
        assert_eq!(
            again.construction_units(),
            cost,
            "after a budget of {budget}"
        );
        assert_eq!(again.free(), fresh.free());
        assert_eq!(
            format!("{:?}", again.evidence().tables),
            format!("{:?}", fresh.evidence().tables)
        );
        for word in WORDS.iter().chain(&texts) {
            assert_eq!(
                again.admit(word),
                fresh.admit(word),
                "{word} after {budget}"
            );
        }
        let cut = build_tiled_within(&opts, &g, &fresh, tiled_cost / 2);
        assert!(
            !cut.active(),
            "a half budget leaves the tiled stage inactive"
        );
        let tiled = build_tiled(&opts, &g, &again);
        assert_eq!(tiled.construction_units(), tiled_cost);
        assert_eq!(tiled.active(), fresh_tiled.active());
        for word in WORDS.iter().chain(&texts) {
            assert_eq!(tiled.accepts(word), fresh_tiled.accepts(word), "{word}");
        }
    }
}
