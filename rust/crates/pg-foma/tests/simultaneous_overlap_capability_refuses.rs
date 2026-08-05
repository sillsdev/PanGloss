//! Proves the `conformance-staging/edge-cases/simultaneous-subrule-genuine-overlap/` fixture
//! grammar genuinely lands on the `Refuse` side of `simultaneous.subrule-overlap`'s
//! (`SimultaneousSubruleOverlapPredicate`, D3) pairwise-non-overlap line -- the fixture's own load-
//! bearing claim (its `grammar.xml` top comment) that its two `Simultaneous` subrules' right
//! environments (`ncBackOrMid = {u, e}`, `ncMidOrFront = {e, i}`) genuinely intersect (shared member
//! `e`) is checked here directly against the real lowered-span intersection
//! (`crate::lower::spans_overlap`, via `crate::replace::is_fully_supported_shape`), not merely
//! asserted in prose. Companion to `SimultaneousRewrite`'s own **NEEDS-ORACLE** verdict -- this
//! test is the "provably overlap" half of establishing it; the oracle-comparison half lives in the
//! fixture's own `STAGING.md` (hc.dll ground truth vs. `pg_parse::Morpher`, run by hand, not by
//! this test -- this crate does not depend on `pg-parse`).

use pg_foma::replace::is_fully_supported_shape;
use pg_grammar::model::PhonRuleDef;

fn fixture_xml() -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../../conformance-staging/edge-cases/simultaneous-subrule-genuine-overlap/grammar.xml",
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn fixture_grammar_subrules_genuinely_overlap_and_are_refused() {
    let xml = fixture_xml();
    let g = pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"));

    assert_eq!(
        g.prules.len(),
        1,
        "fixture must declare exactly one PhonologicalRule"
    );
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(
        rule.mode,
        pg_grammar::model::RewriteMode::Simultaneous,
        "fixture's rule must carry multipleApplicationOrder=\"simultaneous\""
    );
    assert_eq!(
        rule.subrules.len(),
        2,
        "fixture must declare exactly two subrules"
    );
    assert!(
        !rule.subrules[0].self_opaquing && !rule.subrules[1].self_opaquing,
        "neither subrule's RHS pin (featVoice only) constrains featPlace, so both must be \
         non-self-opaquing -- if this ever trips, the overlap witness below would be masked by \
         the self_opaquing early-out instead of the genuine lowered-span intersection this test \
         means to exercise"
    );

    assert!(
        !is_fully_supported_shape(&g, rule),
        "the fixture's two subrules' right environments (ncBackOrMid = {{u, e}}, ncMidOrFront = \
         {{e, i}}) share member 'e' -- D3's real lowered-span intersection (crate::lower::\
         spans_overlap) must find this a genuine overlap and refuse compilation, exactly like \
         phase_c_simultaneous.rs's own SIM_OVERLAP_XML case. If this assertion ever flips, either \
         the fixture stopped exercising a genuine overlap (author error) or D3's own predicate \
         changed what it admits (in which case this fixture's STAGING.md oracle-comparison verdict \
         needs re-examination, not just this test's expectation flipped)."
    );
}
