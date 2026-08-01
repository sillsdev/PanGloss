//! Task 6.1 evidence: the Divvun `lang-sme` derivation-order filter uses flags only as
//! inserted output of context-free left-arrow replace rules.

use foma::apply::apply_init;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;

const DIVVUN_DERIVATION_ORDER: &str = r#"
"@D.Der1.TRUE@" "@D.Der2.TRUE@" "@P.Der1.TRUE@" "+Der1" <- "+Der1" ,
                "@D.Der2.TRUE@" "@P.Der2.TRUE@" "+Der2" <- "+Der2" ;
"#;

fn apply_down_all(net: &Fsm, input: &str) -> Vec<String> {
    let mut handle = apply_init(net);
    handle.down(input).collect()
}

#[test]
fn divvun_inserted_flags_accept_ascending_and_reject_descending_derivations() {
    let net = fsm_parse_regex(&FomaOptions::default(), DIVVUN_DERIVATION_ORDER, None, None)
        .expect("the exact context-free Divvun-style filter must compile");

    // For `A <- B`, downward application presents B on the input tape and evaluates
    // the inserted flags on the upper tape. Upward application would emit those flags
    // as output without checking them; that direction-specific behavior is part of the
    // apply-time caveat, not evidence that inserted flags are universally transparent.
    let ascending = apply_down_all(&net, "+Der1+Der2");
    assert!(
        !ascending.is_empty(),
        "Der1-before-Der2 must survive inserted flag checks: {ascending:?}"
    );

    let descending = apply_down_all(&net, "+Der2+Der1");
    assert!(
        descending.is_empty(),
        "Der2-before-Der1 must be rejected by inserted flag checks: {descending:?}"
    );
}
