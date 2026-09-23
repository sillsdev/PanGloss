use super::*;

#[test]
fn auto_completes_xample_authored_or_explicitly_accepted_unspecified_graphemes() {
    assert_eq!(
        SubstratePolicy::Auto.resolve(ActiveParser::XAmple, false),
        ResolvedSubstratePolicy::CompleteFromUsage
    );
    assert_eq!(
        SubstratePolicy::Auto.resolve(ActiveParser::XAmple, true),
        ResolvedSubstratePolicy::CompleteFromUsage
    );
    assert_eq!(
        SubstratePolicy::Auto.resolve(ActiveParser::Hc, true),
        ResolvedSubstratePolicy::CompleteFromUsage
    );
    assert_eq!(
        SubstratePolicy::Auto.resolve(ActiveParser::Hc, false),
        ResolvedSubstratePolicy::Strict
    );
}

#[test]
fn strict_and_complete_from_usage_are_fixed_points_regardless_of_the_two_facts() {
    for (active_parser, accept) in [
        (ActiveParser::XAmple, false),
        (ActiveParser::XAmple, true),
        (ActiveParser::Hc, false),
        (ActiveParser::Hc, true),
    ] {
        assert_eq!(
            SubstratePolicy::Strict.resolve(active_parser, accept),
            ResolvedSubstratePolicy::Strict
        );
        assert_eq!(
            SubstratePolicy::CompleteFromUsage.resolve(active_parser, accept),
            ResolvedSubstratePolicy::CompleteFromUsage
        );
    }
}

/// No `..` rest pattern -- a new field here fails to compile until this test names it too.
#[test]
fn compile_options_carries_exactly_substrate_and_semantic_loss() {
    let CompileOptions {
        substrate,
        semantic_loss,
    } = CompileOptions::default();
    assert_eq!(substrate, SubstratePolicy::Auto);
    assert_eq!(semantic_loss, SemanticLossPolicy::Refuse);
}
