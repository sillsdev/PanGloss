//! THROWAWAY diagnostic: prints one word's per-backend proposals vs. the oracle's expected set.

use pg_conformance_fixtures::{discover, Root};
use pg_foma::backend_runtime::{
    evaluate_plans_observed_with_cache, RunEvaluationCache, RuntimeBudget,
};
use pg_foma::enumerate::{enumerate_default, CandidateRole, EmissionStrategy, LoweredCandidate};
use pg_foma::junctions::PhonologyProbe;
use pg_foma::lowering_adapter::LoweringAdapter;
use pg_foma::replace::SegAlphabet;

fn main() {
    let root_arg = std::env::args()
        .nth(1)
        .expect("usage: zz_containment_probe <machine|staging> <fixture-name> <word>");
    let name = std::env::args().nth(2).expect("fixture name");
    let target_word = std::env::args().nth(3).expect("word");

    let root = match root_arg.as_str() {
        "machine" => Root::Machine,
        "staging" => Root::Staging,
        other => panic!("unknown root {other}"),
    };

    let fixture = discover()
        .into_iter()
        .find(|f| f.root == root && f.name == name)
        .unwrap_or_else(|| panic!("fixture not found: {root_arg}/{name}"));
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("grammar must load");
    let words: Vec<String> = fixture
        .load_words_yaml()
        .words
        .into_iter()
        .map(|w| w.word)
        .collect();
    println!("fixture words: {words:?}");

    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let prules: Vec<&pg_grammar::model::PhonRuleDef> = grammar
        .strata
        .iter()
        .flat_map(|stratum| {
            stratum
                .prules
                .iter()
                .map(|id| &grammar.prules[id.0 as usize])
        })
        .collect();
    let baseline_plan = enumerate_default(
        &grammar,
        &alphabet,
        &prules,
        PhonologyProbe::new(&grammar).as_ref(),
    );

    let strategies = [
        EmissionStrategy::PlanComposed,
        EmissionStrategy::TunedSurfaceProbed,
        EmissionStrategy::TemplatedUnderlyingTokens,
    ];
    let plans: Vec<LoweredCandidate> = strategies
        .iter()
        .map(|&strategy| LoweredCandidate {
            label: "zz-probe",
            plan: baseline_plan.clone(),
            adapter: LoweringAdapter::for_strategy(strategy),
            role: if strategy == EmissionStrategy::PlanComposed {
                CandidateRole::Baseline
            } else {
                CandidateRole::Alternative
            },
        })
        .collect();

    let mut cache = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
        .unwrap_or_else(|e| panic!("oracle prep faulted: {e}"));
    let observed = evaluate_plans_observed_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget::default(),
        &mut cache,
    );

    for (plan, observation) in plans.iter().zip(&observed) {
        println!("=== backend {:?} ===", plan.strategy());
        let Some(evidence) = &observation.words else {
            println!(
                "  evaluation FAILED outright: {:?}",
                observation.evaluation.certification
            );
            continue;
        };
        let Some(word_ev) = evidence.iter().find(|w| w.word == target_word) else {
            println!("  word {target_word:?} not in comparable evidence (excluded by oracle?)");
            continue;
        };
        println!("  expected (oracle):");
        for a in &word_ev.expected {
            println!(
                "    morphemes={:?} root_index={}",
                a.morpheme_ids, a.root_morpheme_index
            );
        }
        println!("  proposals (final candidate vector sent to confirm):");
        for c in &word_ev.proposals {
            let ids: Vec<u32> = c.morphemes.iter().map(|m| m.0).collect();
            println!("    morphemes={:?} root_index={}", ids, c.root_index);
        }
        println!("  actual (post-confirm):");
        for a in &word_ev.actual {
            println!(
                "    morphemes={:?} root_index={}",
                a.morpheme_ids, a.root_morpheme_index
            );
        }
    }
}
