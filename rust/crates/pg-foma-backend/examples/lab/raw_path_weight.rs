//! Totals one phase (startup, propose, analyze) of the foma pipeline per process, so a PMC trace can price a raw proposer path against a confirmation work step.

use pg_foma::composite::FomaAnalyzer;

pub fn main(args: &[String]) {
    let [grammar_path, words_path, phase] = args else {
        eprintln!("usage: lab raw_path_weight <grammar.xml> <words.txt> startup|propose|analyze");
        std::process::exit(2);
    };
    let xml = std::fs::read_to_string(grammar_path).expect("read grammar");
    let grammar = pg_grammar::load(&xml).expect("load grammar");
    let words: Vec<String> = std::fs::read_to_string(words_path)
        .expect("read words")
        .lines()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect();
    let proposer = pg_foma::analyzer::compile_proposer(&grammar).expect("compile the proposer");
    let mut analyzer = FomaAnalyzer::from_precompiled_proposer(&grammar, proposer);
    match phase.as_str() {
        "startup" => println!("PHASE startup words=0"),
        "propose" => {
            let proposed = analyzer.propose_words(&words);
            let candidates: usize = proposed.iter().map(|p| p.candidates().len()).sum();
            println!(
                "PHASE propose words={} candidates={candidates}",
                words.len()
            );
        }
        "analyze" => {
            let (mut raw_paths, mut confirmation_work, mut calls) = (0usize, 0usize, 0usize);
            for word in &words {
                let profiled = analyzer.analyze_word_with_diagnostics(word);
                raw_paths += profiled.diagnostics.raw_paths;
                confirmation_work += profiled.diagnostics.confirmation_work;
                calls += profiled.diagnostics.confirmation_calls;
            }
            println!(
                "PHASE analyze words={} raw_paths={raw_paths} confirmation_work={confirmation_work} confirmation_calls={calls}",
                words.len()
            );
        }
        other => {
            eprintln!("unknown phase {other}; expected startup, propose or analyze");
            std::process::exit(2);
        }
    }
}
