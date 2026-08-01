//! Task 6.1 evidence: a minimal exact-shaped projection of the Divvun `lang-sme`
//! derivation-order filter, with direction-specific apply-time checks.

use std::collections::BTreeSet;

use foma::apply::{apply_init, apply_set_obey_flags};
use foma::constructions::fsm_invert;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;

const MINIMAL_EXACT_SHAPED_PROJECTION: &str = r#"
"@D.Der1.TRUE@" "@D.Der2.TRUE@" "@P.Der1.TRUE@" "+Der1" <- "+Der1" ,
                "@D.Der2.TRUE@" "@P.Der2.TRUE@" "+Der2" <- "+Der2" ;
"#;

fn apply_down_all(net: &Fsm, input: &str) -> BTreeSet<String> {
    let mut handle = apply_init(net);
    handle.down(input).collect()
}

fn apply_up_all(net: &Fsm, input: &str, obey_flags: bool) -> BTreeSet<String> {
    let mut handle = apply_init(net);
    apply_set_obey_flags(&mut handle, obey_flags as i32);
    handle.up(input).collect()
}

fn exact_output(output: &str) -> BTreeSet<String> {
    [output.to_owned()].into_iter().collect()
}

#[test]
fn minimal_exact_shaped_projection_is_downward_only() {
    let net = fsm_parse_regex(
        &FomaOptions::default(),
        MINIMAL_EXACT_SHAPED_PROJECTION,
        None,
        None,
    )
        .expect("the exact context-free Divvun-style filter must compile");

    // For `A <- B`, downward application presents B on the input tape and evaluates
    // the inserted flags on the upper tape. This is therefore downward-only evidence:
    // under apply_up the same upper-only flags are emitted/suppressed rather than acting
    // as causal checks on the production-direction projection. The proven apply_up
    // construction below uses explicit relation inversion.
    assert_eq!(
        apply_down_all(&net, "+Der1+Der2"),
        exact_output("+Der1+Der2"),
        "downward evidence must accept exactly the ascending projection"
    );

    assert_eq!(
        apply_down_all(&net, "+Der2+Der1"),
        BTreeSet::new(),
        "downward evidence must reject the descending projection"
    );
}

#[test]
fn minimal_exact_shaped_projection_inversion_is_apply_up_safe() {
    let opts = FomaOptions::default();
    let net = fsm_parse_regex(&opts, MINIMAL_EXACT_SHAPED_PROJECTION, None, None)
        .expect("the exact context-free Divvun-style filter must compile");
    let inverted = fsm_invert(net);

    assert_eq!(
        apply_up_all(&inverted, "+Der1+Der2", true),
        exact_output("+Der1+Der2"),
        "inversion must accept exactly the ascending Der1/Der2 projection"
    );
    assert_eq!(
        apply_up_all(&inverted, "+Der2+Der1", true),
        BTreeSet::new(),
        "inversion must reject the descending Der2/Der1 projection"
    );
    assert_eq!(
        apply_up_all(&inverted, "+Der2+Der1", false),
        exact_output("+Der2+Der1"),
        "disabling flag obedience must make the descending projection reachable"
    );
}
