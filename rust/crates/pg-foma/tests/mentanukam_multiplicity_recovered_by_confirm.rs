//! "mentanukam" (machine:edge-cases/mpr-gated-exception) has two derivation orders sharing one
//! identity; confirm's D4 recovery restores both from one deduplicated proposal. See `docs/adr/0001-honest-capability-boundary.md`.

use std::fs;
use std::path::Path;

use pg_foma::backend_runtime::{
    evaluate_plans_observed_with_cache, word_proposal_containment, RunEvaluationCache,
    RuntimeBudget,
};
use pg_foma::enumerate::{enumerate_default, CandidateRole, EmissionStrategy, LoweredCandidate};
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::lowering_adapter::LoweringAdapter;

fn load() -> pg_grammar::model::Grammar {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../machine/conformance/edge-cases/mpr-gated-exception/grammar.xml");
    let xml = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

#[test]
fn mentanukam_proposes_once_but_confirms_the_oracles_full_multiplicity() {
    let g = load();
    let words = vec!["mentanukam".to_string()];
    let semantics = GrammarSemantics::derive(&g);
    let phonology = PhonologyProbe::new_with_semantics(&semantics);
    let baseline_plan = enumerate_default(&g, semantics.prules_in_order(), phonology.as_ref());

    for strategy in [
        EmissionStrategy::TunedSurfaceProbed,
        EmissionStrategy::TemplatedUnderlyingTokens,
    ] {
        let candidate = LoweredCandidate {
            label: "mentanukam-multiplicity",
            plan: baseline_plan.clone(),
            adapter: LoweringAdapter::for_strategy(strategy),
            role: CandidateRole::Alternative,
        };
        let mut cache = RunEvaluationCache::prepare(&g, &words, RuntimeBudget::default())
            .unwrap_or_else(|e| panic!("{strategy:?}: oracle prep faulted: {e}"));
        let observed = evaluate_plans_observed_with_cache(
            &g,
            std::slice::from_ref(&candidate),
            &words,
            RuntimeBudget::default(),
            &mut cache,
        );
        let evidence = observed[0]
            .words
            .as_ref()
            .unwrap_or_else(|| panic!("{strategy:?}: no per-word evidence produced"));
        assert_eq!(evidence.len(), 1, "{strategy:?}: exactly one word measured");
        let word = &evidence[0];

        assert_eq!(
            word.expected.len(),
            2,
            "{strategy:?}: the oracle must require this identity twice (two derivation orders)"
        );
        assert_eq!(
            word.proposals.len(),
            1,
            "{strategy:?}: the proposer deduplicates the two derivation orders into one candidate \
             pre-confirm -- if this changes, the multiplicity-recovery claim this test pins may no \
             longer be exercised"
        );
        assert_eq!(
            word.actual.len(),
            2,
            "{strategy:?}: confirm_all's D4 multiplicity recovery must restore both derivations \
             from the single deduplicated candidate"
        );
        assert_eq!(
            word_proposal_containment(word),
            Ok(()),
            "{strategy:?}: presence containment must hold even though pre-confirm multiplicity (1) \
             is below the oracle's own (2)"
        );
        assert!(
            matches!(
                observed[0].evaluation.certification,
                pg_foma::backend_optimizer::Certification::FullHcConfirmed { .. }
            ),
            "{strategy:?}: the end-to-end pipeline output must be oracle-exact: {:?}",
            observed[0].evaluation.certification
        );
    }
}
