//! Candidate-set parity harness for two verified propose-phase micro-optimizations:
//!   - Fix 1: `apply_append`'s dead-string-allocation removal (skip building `bstring`/`sep` on
//!     branches that never read them, and push display strings straight into `h.outstring`
//!     instead of through an intermediate `String` where possible) — applied to the
//!     since-retired `rust/vendor/foma/src/apply.rs` and independently present in the official
//!     `foma` crate as of the 0.4.0 release this repo now depends on directly.
//!   - Fix 2: `rust/crates/pg-foma/src/tags.rs`'s `decode_path` rewrite from a `Vec<char>` scan to
//!     direct byte/`&str` slicing.
//!
//! Dumps, for EVERY word of all three sample corpora, every `FomaProposer::propose` candidate
//! (morpheme ids + root_index) IN ORDER, to stdout in a deterministic line format. Run once at
//! baseline (e.g. `git stash`) and once with the changes applied; the two dumps must be
//! byte-identical -- that identity is the actual correctness gate for both optimizations (neither
//! is meant to change propose's observable output, only its allocation pattern).
//!
//! Not a `cargo test` -- run manually and diff:
//!   cargo run -p pg-foma --release --example propose_parity > /tmp/before.txt
//!   ... apply changes ...
//!   cargo run -p pg-foma --release --example propose_parity > /tmp/after.txt
//!   diff /tmp/before.txt /tmp/after.txt

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

/// Amharic's deep composite/rule-chain recursion needs a bigger stack than the default main thread
/// gets under a release build's larger inlined frames (mirrors `examples/precision_bench.rs`'s same
/// spawn trick).
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
