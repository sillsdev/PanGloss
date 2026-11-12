use super::tests::{assimilation_spec, grammar_xml, load, prefix, spec, suffix, with_late_stratum};
use super::tiled_tests::{setup, Setup};
use super::*;

fn grammars() -> Vec<(&'static str, Grammar)> {
    let plain = load(&spec(
        "kaqtnu",
        vec![prefix(0, "ka"), suffix(1, "ta"), suffix(2, "nu")],
        &["qt", "tq"],
    ));
    let phonology = load(&assimilation_spec(
        "pmnatku",
        vec![suffix(0, "na"), suffix(1, "ku")],
        &["tap", "tat"],
    ));
    let xml = grammar_xml(&assimilation_spec(
        "pmnatku",
        vec![suffix(0, "na")],
        &["tap", "tat"],
    ));
    let outer = with_late_stratum(xml, &[9], &[suffix(9, "ku")]);
    vec![("plain", plain), ("phonology", phonology), ("outer", outer)]
}

#[test]
fn the_floors_are_at_most_the_units_the_builds_spend() {
    for (name, g) in grammars() {
        let Setup { admission, tiled } = setup(&g);
        let floor = admission.plan().construction_floor(&g);
        assert!(floor > 0, "{name}");
        assert!(floor <= admission.construction_units(), "{name}: {floor}");
        let prospect = tiled_prospect(&g, &admission);
        assert!(tiled.active(), "{name}");
        assert!(
            prospect.construction_floor() <= tiled.construction_units(),
            "{name}: {}",
            prospect.construction_floor()
        );
    }
}

#[test]
fn a_budget_under_the_admission_floor_refuses_with_nothing_spent() {
    let opts = FomaOptions::default();
    for (name, g) in grammars() {
        let floor = plan_admission(&g, &class_of(&g))
            .expect("plans")
            .construction_floor(&g);
        match build_rule_admission_within(&opts, &g, floor - 1) {
            Err(AdmissionRefusal::ConstructionBudget { spent, budget }) => {
                assert_eq!((spent, budget), (0, floor - 1), "{name}");
            }
            other => panic!("{name}: {:?}", other.map(|_| ())),
        }
        let full = build_rule_admission(&opts, &g).expect("builds");
        let within = build_rule_admission_within(&opts, &g, full.construction_units());
        assert!(within.is_ok(), "{name}");
    }
}

#[test]
fn a_budget_under_the_tiled_floor_refuses_with_nothing_spent() {
    let opts = FomaOptions::default();
    for (name, g) in grammars() {
        let Setup { admission, tiled } = setup(&g);
        let floor = tiled_prospect(&g, &admission).construction_floor();
        assert!(floor > 0, "{name}");
        let short = build_tiled_within(&opts, &g, &admission, floor - 1);
        assert!(!short.active(), "{name}");
        assert_eq!(short.construction_units(), 0, "{name}");
        assert!(short.evidence().budget_exhausted, "{name}");
        let reason = short.inactive_reason().expect("a reason");
        assert!(reason.contains("construction-budget:0 of"), "{reason}");
        let enough = build_tiled_within(&opts, &g, &admission, tiled.construction_units());
        assert!(enough.active(), "{name}");
    }
}

#[test]
fn the_floor_counts_the_pieces_of_both_regions() {
    let g = &grammars()[2].1;
    let plan = plan_admission(g, &class_of(g)).expect("plans");
    assert!(!plan.outer_pieces.is_empty());
    let one = AdmissionPlan {
        outer_pieces: Vec::new(),
        ..plan.clone()
    };
    assert!(one.construction_floor(g) < plan.construction_floor(g));
}
