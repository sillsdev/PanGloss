//! Ad hoc measurement harness for the `finish_controllable_net` boundary-cleanup precision fix
//! (`docs/fst-plan/large-lexicon-proposal-explosion.md`). NOT a test -- a throwaway-style probe
//! (module doc convention already used by this crate's `boundary_cleanup_precision_probe.rs`,
//! which the investigation that produced the design doc deleted after use; this one stays checked
//! in a while the fix is being re-measured across sessions/agents).
//!
//! Drives ONLY public API (`pg_foma::recipe_runtime::evaluate_plans`, `pg_foma::enumerate::
//! enumerate_default`, `pg_foma::recipe_registry::Registry`) -- never touches `recipe_runtime.rs`/
//! `recipe_optimize.rs` internals, so it is safe to run against a checkout where those files are
//! mid-edit by someone else. Reports ONE word at a time (unlike the `recipe-optimize` CLI, whose
//! aggregate search/pilot/oracle machinery adds cost unrelated to the specific pathology this
//! measures), so a per-word proposal count is directly comparable to the diagnosis doc's own table.
//!
//! Run: `cargo run --release -p pg-foma --example boundary_marker_precision_measure -- \
//!   <grammar.xml> <words.txt>`

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::recipe_runtime::{evaluate_plans, RuntimeBudget};
use pg_foma::replace::SegAlphabet;
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

    // Same word-list hygiene the diagnosis doc's own measurement used: strip `\r` (CRLF source
    // files), drop blank lines. No gloss-header stripping here -- the caller is responsible for
    // pointing this at a file that is already just words (verified by hand for `sena-words.txt`:
    // no leading gloss lines, unlike its Amharic sibling).
    let words: Vec<String> = fs::read_to_string(&words_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", words_path.display()))
        .lines()
        .map(|l| l.trim_end_matches('\r').trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let prules: Vec<_> = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect();
    let phonology = PhonologyProbe::new(&grammar);
    let baseline = enumerate_default(&grammar, &alphabet, &prules, phonology.as_ref());
    let candidates = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar: &grammar,
            baseline: &baseline,
        })
        .expect("materialization must succeed");
    // Baseline only (element zero, module doc's own convention in
    // `recipe_runtime_net_is_queryable_gate.rs`'s `materialize_and_evaluate`): this measures
    // `build_controllable` + `finish_controllable_net`'s own precision, not the recipe optimizer's
    // candidate-plan search space.
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
        );
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
