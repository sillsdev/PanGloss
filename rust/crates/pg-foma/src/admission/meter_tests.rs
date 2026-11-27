use super::meter::Meter;
use super::tests::{assimilation_spec, load, suffix};
use super::*;
use crate::cascade_parts::CascadeParts;

#[test]
fn an_operation_that_does_not_fit_is_not_run_and_ends_the_build() {
    let opts = FomaOptions::default();
    let probe = Meter::default();
    probe.symbol("a");
    let symbol = probe.units();
    let meter = Meter::within(4 * symbol - 1);
    let (a, b) = (meter.symbol("a"), meter.symbol("b"));
    assert_eq!(meter.units(), 2 * symbol);
    let both = meter.union(&opts, a, b);
    assert_eq!(both.arccount, 0, "the union was not made");
    assert_eq!(
        meter.units(),
        2 * symbol,
        "an operation that did not run is not counted"
    );
    assert!(matches!(
        meter.check(0),
        Err(AdmissionRefusal::ConstructionBudget { .. })
    ));
    let c = meter.symbol("c");
    let more = meter.star(&opts, c);
    assert_eq!(more.arccount, 0);
    assert!(!meter.equivalent(&opts, &more, &more));
    assert_eq!(meter.units(), 2 * symbol);
    assert!(!meter.allows(0));
}

#[test]
fn an_operation_that_fits_runs_and_counts_its_operands_and_result() {
    let opts = FomaOptions::default();
    let meter = Meter::default();
    let (a, b) = (meter.symbol("a"), meter.symbol("b"));
    let before = meter.units();
    let both = meter.union(&opts, a, b);
    assert!(both.arccount >= 2);
    assert!(meter.units() > before);
    assert!(meter.check(0).is_ok() && meter.allows(u64::MAX / 2));
}

#[test]
fn a_cascade_build_counts_its_operations_and_stops_when_refused() {
    let opts = FomaOptions::default();
    let g = load(&assimilation_spec(
        "pmnatku",
        vec![suffix(0, "na"), suffix(1, "ku")],
        &["tap", "tat"],
    ));
    let prules = build::ordered_prules(&g);
    let counted = Meter::default();
    let whole = CascadeParts::build_within(&opts, &g, &prules, &counted).expect("parts");
    let spent = counted.units();
    assert!(!whole.rules.is_empty());
    assert!(
        spent
            > whole
                .rules
                .iter()
                .map(|part| part.net.arccount as u64)
                .sum::<u64>(),
        "the operations behind the rules are counted, not only the rules"
    );
    let plain = CascadeParts::build(&opts, &g, &prules).expect("parts");
    assert_eq!(plain.rules.len(), whole.rules.len());
    for (a, b) in plain.rules.iter().zip(&whole.rules) {
        assert_eq!(
            (a.net.statecount, a.net.arccount),
            (b.net.statecount, b.net.arccount)
        );
    }
    let tight = Meter::within(spent / 2);
    let partial = CascadeParts::build_within(&opts, &g, &prules, &tight).expect("parts");
    assert!(matches!(
        tight.check(0),
        Err(AdmissionRefusal::ConstructionBudget { .. })
    ));
    assert!(
        tight.units() < spent,
        "the build stopped before its last operation"
    );
    assert!(partial.rules.len() < whole.rules.len() || partial.spelling.arccount == 0);
    assert!(Meter::within(spent).check(0).is_ok());
}

#[test]
fn the_tiled_stage_reads_the_cascade_the_admission_built() {
    let opts = FomaOptions::default();
    let g = load(&assimilation_spec(
        "pmnatku",
        vec![suffix(0, "na"), suffix(1, "ku")],
        &["tap", "tat"],
    ));
    let prules = build::ordered_prules(&g);
    let counted = Meter::default();
    CascadeParts::build_within(&opts, &g, &prules, &counted).expect("parts");
    let mut admission = build_rule_admission(&opts, &g).expect("admission");
    assert!(admission.cascade.is_some());
    let shared = build_tiled(&opts, &g, &admission).construction_units();
    admission.cascade = None;
    let rebuilt = build_tiled(&opts, &g, &admission).construction_units();
    assert_eq!(rebuilt, shared + counted.units());
}

#[test]
fn a_rule_is_charged_before_foma_parses_it_and_not_run_when_it_does_not_fit() {
    let opts = FomaOptions::default();
    let rule = "a -> b || c _ d";
    let plain = "[a | b | c]*";
    assert!(
        meter::regex_units(rule) > 100 * meter::regex_units(plain),
        "a replace rule costs far more than a union of the same length"
    );
    let meter = Meter::within(meter::regex_units(rule) - 1);
    let net = meter
        .regex(&opts, rule)
        .expect("a refused parse returns the empty net");
    assert_eq!(net.arccount, 0);
    assert_eq!(meter.units(), 0, "a parse that did not run is not counted");
    assert!(meter.check(0).is_err());
    let fits = Meter::within(meter::regex_units(rule) + 1000);
    let net = fits.regex(&opts, rule).expect("parses");
    assert!(net.arccount > 0);
    assert!(fits.units() >= meter::regex_units(rule));
    assert!(fits.check(0).is_ok());
}
