//! Measures `finish_controllable_net`'s boundary-cleanup precision, one word at a time, via public API only.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use pg_foma::backend_registry::{MaterializerContext, Registry};
use pg_foma::backend_runtime::{evaluate_plans, RuntimeBudget};
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_grammar::model::Grammar;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: boundary_marker_precision_measure <grammar.xml> <words.txt>");
        std::process::exit(2);
    }
    let grammar_path = PathBuf::from(&args[1]);
    let words_path = PathBuf::from(&args[2]);

    let xml = fs::read_to_string(&grammar_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", grammar_path.display()));
    let grammar: Grammar =
        pg_grammar::load(&xml).unwrap_or_else(|e| panic!("grammar load failed: {e}"));

    // Strips CRLF `\r` and blank lines; caller must pre-strip any gloss-header lines from the word list.
    let words: Vec<String> = fs::read_to_string(&words_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", words_path.display()))
        .lines()
        .map(|l| l.trim_end_matches('\r').trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    let prules: Vec<_> = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect();
    let phonology = PhonologyProbe::new(&grammar);
    let baseline = enumerate_default(&grammar, &prules, phonology.as_ref());
    let candidates = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar: &grammar,
            baseline: &baseline,
        })
        .expect("materialization must succeed");
    // Baseline candidate only (index 0): measures boundary-cleanup precision, not plan search space.
    let plans: Vec<_> = candidates.into_iter().map(|(_, p)| p).collect();
    assert!(!plans.is_empty(), "must materialize at least one candidate");
    let baseline_plan = std::slice::from_ref(&plans[0]);

    println!("word,proposals,confirmation_calls,states,arcs,certification,elapsed_ms");
    let mut total_proposals: u64 = 0;
    for w in &words {
        let t = Instant::now();
        let evaluations = evaluate_plans(
            &grammar,
            baseline_plan,
            std::slice::from_ref(w),
            RuntimeBudget::default(),
        )
        .expect("the oracle liveness net / memory ceiling must not trip on this fixture");
        let elapsed_ms = t.elapsed().as_millis();
        let e = &evaluations[0];
        total_proposals += e.score.proposals;
        println!(
            "{w},{},{},{},{},{:?},{elapsed_ms}",
            e.score.proposals, e.score.confirmation, e.score.states, e.score.arcs, e.certification
        );
    }
    println!("TOTAL,{total_proposals}");
}
