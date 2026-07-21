//! Throwaway P6/Aweti diagnostic (task brief Q1): is the composed
//! `lexc(templated) .o. rules .o. boundary_cleanup` network CYCLIC or ACYCLIC?
//!
//! Uses the vendored foma-rs's own `foma::topsort::fsm_topsort` (Kahn's-algorithm topological
//! sort + path counter, `foma-0.4.0/src/topsort.rs`) rather than a hand DFS: it is a plain linear
//! pass over the state/arc line table (no backtracking search, no `apply_up`), so it is SAFE to run
//! directly on the full composed network regardless of the `apply_up` hang this whole P6-Aweti
//! investigation is about. `fsm_topsort` sets `net.is_loop_free`/`net.pathcount` (`PATHCOUNT_CYCLIC`
//! = -1 iff a cycle -- including a self-loop or a back-edge into an already-topologically-treated
//! state -- was found during the single linear pass); it does not enumerate paths.
//!
//! Mirrors `examples/p6_aweti_replace_prototype.rs`'s own compose flow exactly (same emitter, same
//! rule cascade, same boundary cleanup) so the network under test here is byte-identical to the one
//! `tests/p6_aweti_gate.rs` exercises.
//!
//! Run: `cargo run --release -p pg-foma --example p6_aweti_q1_cycle_check`

use std::path::{Path, PathBuf};
use std::time::Instant;

use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::topsort::fsm_topsort;
use foma::types::{Tern, PATHCOUNT_CYCLIC, PATHCOUNT_OVERFLOW};

use pg_foma::emit::emit_underlying_templated;
use pg_foma::replace::{compile_and_compose_rules, SegAlphabet};
use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::{Grammar, PhonRuleDef};

const STACK_BYTES: usize = 512 * 1024 * 1024;

fn sample_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/data")
        .join(name)
}

fn load_aweti() -> Grammar {
    let path = sample_path("aweti.json");
    let json =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let snapshot = pg_snapshot::Snapshot::from_json(&json)
        .unwrap_or_else(|e| panic!("parse snapshot {}: {e}", path.display()));
    let (grammar, _warnings) = pg_grammar::compile_project(&snapshot)
        .unwrap_or_else(|e| panic!("compile_project {}: {e}", path.display()));
    grammar
}

fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run)
        .expect("spawn large-stack worker thread");
    handle.join().expect("worker thread panicked");
}

fn run() {
    println!("=== P6 Aweti Q1: is the composed network cyclic? ===\n");
    let g = load_aweti();
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();

    let result = emit_underlying_templated(&g, &alphabet, None);
    println!("emit tier: {:?}", result.report.tier);
    let lexc_net = fsm_lexc_parse_string(&opts, None, &result.lexc_source)
        .unwrap_or_else(|| panic!("templated underlying-form lexc failed to compile"));
    println!(
        "lexc net: {} states, {} arcs",
        lexc_net.statecount, lexc_net.arccount
    );

    let mut rules_in_order: Vec<&PhonRuleDef> = Vec::new();
    for st in &g.strata {
        for &prid in &st.prules {
            rules_in_order.push(&g.prules[prid.0 as usize]);
        }
    }
    let mut skipped_rules: Vec<String> = Vec::new();
    let mut tuple_reports = Vec::new();
    let rule_net = compile_and_compose_rules(
        &opts,
        &g,
        &alphabet,
        &rules_in_order,
        &mut skipped_rules,
        &mut tuple_reports,
    )
    .expect("compose budget ok")
    .expect("Aweti's 18 rules must compile");
    println!(
        "rule net: {} states, {} arcs; skipped={skipped_rules:?}",
        rule_net.statecount, rule_net.arccount
    );

    let boundary_tokens: Vec<char> = table
        .iter()
        .filter(|(_, cd)| cd.kind() == CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect();
    let cleanup_regex = boundary_tokens
        .iter()
        .map(|c| format!("{c} -> 0"))
        .collect::<Vec<_>>()
        .join(", ");
    let cleanup_net = fsm_parse_regex(&opts, &cleanup_regex, None, None)
        .unwrap_or_else(|| panic!("boundary cleanup regex failed to compile"));

    let t_compose = Instant::now();
    let composed = fsm_compose(&opts, lexc_net, rule_net);
    let composed = fsm_compose(&opts, composed, cleanup_net);
    let composed = fsm_minimize(&opts, composed);
    println!(
        "\nfull composition + minimize: {:?}; final net: {} states, {} arcs",
        t_compose.elapsed(),
        composed.statecount,
        composed.arccount
    );

    // --- The actual Q1 test: fsm_topsort is a LINEAR pass (no apply_up, no backtracking search) --
    let t_topsort = Instant::now();
    let sorted = fsm_topsort(composed);
    println!("\nfsm_topsort: {:?}", t_topsort.elapsed());
    println!("is_loop_free = {:?}", sorted.is_loop_free);
    match sorted.pathcount {
        PATHCOUNT_CYCLIC => println!("pathcount = PATHCOUNT_CYCLIC ({PATHCOUNT_CYCLIC}) -- THE NETWORK IS CYCLIC"),
        PATHCOUNT_OVERFLOW => println!(
            "pathcount = PATHCOUNT_OVERFLOW ({PATHCOUNT_OVERFLOW}) -- network is ACYCLIC but has more \
             than i64::MAX accepting paths (still a FINITE language, just an astronomically large one)"
        ),
        n => println!("pathcount = {n} -- network is ACYCLIC with exactly {n} accepting paths (finite language)"),
    }
    assert_eq!(
        sorted.is_loop_free,
        if matches!(sorted.pathcount, PATHCOUNT_CYCLIC) {
            Tern::No
        } else {
            Tern::Yes
        }
    );

    println!("\n=== done ===");
}
