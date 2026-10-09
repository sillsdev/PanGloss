use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use pg_conformance_fixtures::require_fixture;
use pg_foma::analyzer::FomaProposer;
use pg_foma::capability::{
    characterize, CapabilityPredicate, PredicateVerdict, QuantifierBoundedExpansionPredicate,
    RightToLeftRewriteFaithfulReversalPredicate,
};
use pg_foma::compose_budget::ApplyBudget;
use pg_foma::composite::{FomaAnalyzer, ProfiledFomaApplyOutcome};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::plan::{FragmentSpec, PlanNodeKind, Provenance};
use pg_foma::replace::{compile_and_compose_rules, SegAlphabet};
use pg_foma::uflexc::emit_underlying_filtered;
use pg_foma_backend::backend_selection::select_backends;
use pg_grammar::model::{Dir, PRuleId, PhonRuleDef};

pub(super) fn assert_capabilities(name: &str, has_quantifier: bool) {
    let fixture = require_fixture("edge-cases", name);
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    assert!(
        select_backends(&GrammarSemantics::derive(&grammar))
            .report_for(EmissionStrategy::TemplatedUnderlyingTokens)
            .expect("templated report")
            .can_represent(),
        "{name}"
    );
    let profile = characterize(&grammar);
    let rule = PRuleId(0);
    let node = PlanNodeKind::Leaf {
        fragment: FragmentSpec::RewriteRule { rule },
        provenance: Provenance::RewriteRule(rule),
    };
    assert_eq!(profile.quantifier_detail(rule).is_some(), has_quantifier);
    if has_quantifier {
        assert_eq!(
            QuantifierBoundedExpansionPredicate.evaluate(&grammar, &profile, &node),
            PredicateVerdict::ConfirmOnly,
            "{name}"
        );
    }
    if matches!(&grammar.prules[0], PhonRuleDef::Rewrite(rule) if rule.dir == Dir::RightToLeft) {
        assert_eq!(
            RightToLeftRewriteFaithfulReversalPredicate.evaluate(&grammar, &profile, &node),
            PredicateVerdict::ConfirmOnly,
            "{name}"
        );
    }
}

pub(super) fn verify_owner(name: &str) -> (usize, usize, usize) {
    let budget = ApplyBudget::with_caps(Some(128), Some(32));
    let mut checked = 0;
    let mut positives = 0;
    let mut pruned = 0;
    let fixture = require_fixture("edge-cases", name);
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    let table = &grammar.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let options = FomaOptions::default();
    let emitted = emit_underlying_filtered(&grammar, &alphabet, None).unwrap();
    assert!(emitted.skipped.is_empty());
    let lexicon = fsm_lexc_parse_string(&options, None, &emitted.lexc_source).unwrap();
    let rules = grammar.strata[0]
        .prules
        .iter()
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let mut skipped = Vec::new();
    let mut reports = Vec::new();
    let rewrite = compile_and_compose_rules(
        &options,
        &grammar,
        &alphabet,
        &rules,
        &mut skipped,
        &mut reports,
    )
    .unwrap_or_else(|| panic!("{name}: owning rewrite compiler refused"));
    assert!(skipped.is_empty(), "{name}: {skipped:?}");
    let network = fsm_minimize(&options, fsm_compose(&options, lexicon, rewrite));
    let proposer = FomaProposer::from_precompiled_network_without_emit_report(&network)
        .with_segment_query_encoder(table);
    let mut analyzer = FomaAnalyzer::from_precompiled_proposer(&grammar, proposer);
    for word in fixture.load_words_yaml().words {
        assert!(word.adapter_visible() && !word.expect_skip);
        let ProfiledFomaApplyOutcome::Complete(profiled) =
            analyzer.analyze_word_with_diagnostics_budgeted(&word.word, &budget)
        else {
            panic!("{name}: {} exceeded the contained apply budget", word.word);
        };
        let outcome = profiled.outcome;
        if outcome.candidates_generated > 0 {
            assert!(
                profiled.diagnostics.confirmation_calls > 0,
                "{name}: {} bypassed HC confirmation",
                word.word
            );
        }
        assert!(outcome.peel_chain_depth_error.is_none());
        assert_eq!(
            pg_parse::result_multiset(&outcome.analyses),
            word.expected_multiset(),
            "{name}: {}",
            word.word
        );
        positives += usize::from(!outcome.analyses.is_empty());
        pruned += usize::from(outcome.candidates_generated > 0 && outcome.confirmed == 0);
        checked += 1;
    }
    (checked, positives, pruned)
}

pub(super) fn verify_templated(name: &str) -> (usize, usize) {
    super::confirmed_rewrite_fixture::verify_templated_fixture(&require_fixture("edge-cases", name))
}
