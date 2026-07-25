//! Follow-up to `deep_chain_scale_probe.rs`: that probe found the bare deep standalone-affix-chain
//! lexc net (no phonological-rule composition) stays fast at every `N` tried, up to N=24 — NOT
//! reproducing the historical Aweti `apply_up` cliff. But the real Aweti gate's own cliff
//! (`docs/fst-plan/p6-deep-truncation-chain-report.md`) was measured on the COMPOSED network
//! (`lexc .o. 18-rule cascade .o. boundary-cleanup`, `pg-foma/tests/p6_aweti_gate.rs`'s own
//! `run_full_corpus_recall`), never on bare lexc alone. This probe isolates that ONE missing
//! variable: compose the SAME N=24 deep-chain lexc net against a trivial (semantically inert)
//! identity rule net via `fsm_compose` + `fsm_minimize` — the same two operations
//! `synthetic-stress-grammar-plan.md` §3's V1/V2 name as the actual blowup vectors ("composition
//! intermediate blowup" / "determinize/minimize exponential worst case") — and re-measures the
//! SAME apply-time probe word against the COMPOSED net, to see whether composition itself (not
//! rule content) is the missing ingredient.
//!
//! Run with `cargo run -p pg-foma --release --example deep_chain_compose_probe`.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use foma::apply::apply_init;
use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;

use pg_foma::emit;
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

const N: usize = 24;
const UNBOUNDED_TIMEOUT: Duration = Duration::from_secs(15);

fn main() {
    let recipe = Recipe {
        name: "deep-chain-compose-probe",
        seed: 20260725,
        scale: ScaleKnobs {
            segment_inventory: N + 1,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 1,
            chain_rule_count: N,
            ..Default::default()
        },
    };
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).expect("generated XML must load");
    let chain = rendered.chain.as_ref().expect("chain_rule_count > 0");

    let opts = FomaOptions::default();
    let emitted = emit::emit(&g);
    let lexc_net = fsm_lexc_parse_string(&opts, None, &emitted.lexc_source)
        .expect("deep-chain lexc must foma-compile");
    println!(
        "bare lexc net: {} states, {} arcs",
        lexc_net.statecount, lexc_net.arccount
    );

    // A trivial, semantically-inert IDENTITY transducer over the whole alphabet (`?*` — foma's
    // "any symbol, either tape, closure" universal identity language): composing against it must
    // not change the recognized relation at all, so any slowdown/blowup this introduces is
    // attributable PURELY to the mechanics of composition/minimization against a deep-chain net,
    // never to any actual rule content (module doc: isolating composition itself as the variable).
    let identity_net =
        fsm_parse_regex(&opts, "?*", None, None).expect("identity regex must compile");

    let t_compose = Instant::now();
    let composed = fsm_compose(&opts, lexc_net, identity_net);
    let composed = fsm_minimize(&opts, composed);
    let compose_elapsed = t_compose.elapsed();
    println!(
        "composed (lexc .o. identity) + minimize: {compose_elapsed:?}; net: {} states, {} arcs",
        composed.statecount, composed.arccount
    );

    // Same maximally-ambiguous probe word as deep_chain_scale_probe.rs: root + every other rule's
    // suffix char, in order (k = N/2 of N, maximizing C(N, k)).
    let k = N / 2;
    let table = &g.char_tables[0];
    let root_ch = chain.root_shape.chars().next().unwrap();
    let mut suffix_chars: Vec<char> = Vec::new();
    for (_, cd) in table.iter() {
        for rep in cd.representations() {
            if let Some(c) = rep.chars().next() {
                if c != root_ch {
                    suffix_chars.push(c);
                }
            }
        }
    }
    suffix_chars.sort_unstable();
    suffix_chars.truncate(N);
    let mut word = chain.root_shape.clone();
    for &c in suffix_chars.iter().take(k) {
        word.push(c);
    }
    println!("probe word: {word:?} (root + {k} of {N} suffixes)");

    let (tx, rx) = mpsc::channel();
    let handle = thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let mut handle = apply_init(&composed);
            let t0 = Instant::now();
            let mut raw_n = 0usize;
            for _ in handle.up(&word) {
                raw_n += 1;
                if raw_n >= 5_000_000 {
                    break; // safety cap on this diagnostic loop itself, not a production budget
                }
            }
            let elapsed = t0.elapsed();
            let _ = tx.send((raw_n, elapsed));
        })
        .expect("spawn probe thread");

    match rx.recv_timeout(UNBOUNDED_TIMEOUT) {
        Ok((raw_n, elapsed)) => println!(
            "apply_up on COMPOSED net: {raw_n} raw results in {elapsed:?} — {}",
            if elapsed > Duration::from_secs(1) {
                "SLOW (composition against even a trivial identity rule materially changed apply cost)"
            } else {
                "fast (composition alone, against this net shape, does not explain the historical cliff either)"
            }
        ),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            println!(
                "apply_up on COMPOSED net: DID NOT COMPLETE within {UNBOUNDED_TIMEOUT:?} — \
                 REPRODUCED (composition against even a trivial identity rule is the missing \
                 ingredient)"
            );
            drop(handle);
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => println!("probe thread panicked"),
    }
}
