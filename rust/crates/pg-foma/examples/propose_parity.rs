//! Candidate-set parity harness: dumps every `FomaProposer::propose` candidate for every word of all three sample corpora, in a deterministic line format. Run once at baseline and once with a propose-phase allocation change applied; the two dumps must diff byte-identical, since such a change is meant to touch only allocation pattern, never propose's observable output.

use std::path::{Path, PathBuf};

use pg_foma::analyzer::FomaProposer;
use pg_foma::tags::Candidate;
use pg_grammar::model::Grammar;

struct GrammarSpec {
    name: &'static str,
    xml_file: &'static str,
    words_file: &'static str,
}

const GRAMMARS: &[GrammarSpec] = &[
    GrammarSpec {
        name: "indonesian",
        xml_file: "indonesian-hc.xml",
        words_file: "indonesian-words.txt",
    },
    GrammarSpec {
        name: "sena",
        xml_file: "sena-hc.xml",
        words_file: "sena-words.txt",
    },
    GrammarSpec {
        name: "amharic",
        xml_file: "amharic-hc.xml",
        words_file: "amharic-words.txt",
    },
];

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn load_grammar(name: &str) -> Option<Grammar> {
    let path = sample_path(name);
    let xml = std::fs::read_to_string(&path).ok()?;
    Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {name}: {e}")))
}

fn read_words(name: &str) -> Option<Vec<String>> {
    let path = sample_path(name);
    let text = std::fs::read_to_string(&path).ok()?;
    Some(
        text.lines()
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

fn fmt_candidate(c: &Candidate) -> String {
    let morphs: Vec<String> = c.morphemes.iter().map(|m| m.0.to_string()).collect();
    format!("[{}]@{}", morphs.join(","), c.root_index)
}

fn run_grammar(spec: &GrammarSpec) {
    let Some(g) = load_grammar(spec.xml_file) else {
        println!("{}\tSKIPPED-no-grammar", spec.name);
        return;
    };
    let Some(words) = read_words(spec.words_file) else {
        println!("{}\tSKIPPED-no-words", spec.name);
        return;
    };
    let mut proposer = match FomaProposer::new(&g) {
        Ok(p) => p,
        Err(e) => {
            println!("{}\tSKIPPED-proposer-error:{}", spec.name, e);
            return;
        }
    };
    for word in &words {
        let cands = proposer.propose(word);
        let rendered: Vec<String> = cands.iter().map(fmt_candidate).collect();
        println!("{}\t{}\t{}", spec.name, word, rendered.join(";"));
    }
}

/// Amharic's deep composite/rule-chain recursion needs a bigger stack than a release build's default main thread gets (same spawn trick as `examples/precision_bench.rs`).
fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(|| {
            for spec in GRAMMARS {
                run_grammar(spec);
            }
        })
        .expect("failed to spawn parity-harness thread");
    handle.join().expect("parity-harness thread panicked");
}
