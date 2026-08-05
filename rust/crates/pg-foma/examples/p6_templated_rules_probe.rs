//! P6 prototype stretch goal: do Aweti's 18 phonological rules compile via `pg_foma::replace`?
//! Scope (per the prototype brief): rule COMPILE only. Does
//! NOT call `pg_foma::emit::emit()` (that is the exact OOM this whole P6 effort routes around —
//! `examples/templated_probe.rs`'s own module doc: 4.9GB RSS, unfinished, in `preexpand::
//! build_composites`) and does NOT attempt underlying-form lexc emission or root-scale
//! composition (`pg_foma::uflexc` is Indonesian-scoped, template-less-morphotactics only; a
//! templated 855-root grammar is out of this prototype's emitter scope entirely, mainline work).
//!
//! Run: `cargo run --release -p pg-foma --example p6_aweti_probe`

use std::path::{Path, PathBuf};
use std::time::Instant;

use foma::options::FomaOptions;

use pg_foma::replace::{compile_and_compose_rules, compile_rewrite_rule, SegAlphabet};
use pg_grammar::model::{Grammar, PhonRuleDef};

const STACK_BYTES: usize = 512 * 1024 * 1024;

fn default_aweti_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../samples/data/aweti.json")
}

fn load_grammar(path: &Path) -> Grammar {
    let json =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let snapshot = pg_snapshot::Snapshot::from_json(&json)
        .unwrap_or_else(|e| panic!("parse snapshot {}: {e}", path.display()));
    let (grammar, warnings) = pg_grammar::compile_project(&snapshot)
        .unwrap_or_else(|e| panic!("compile_project {}: {e}", path.display()));
    if !warnings.is_empty() {
        println!("  ({} compile_project warnings)", warnings.len());
    }
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
    println!("=== P6 stretch goal: Aweti scale probe (rule compile only, no emit/lexc) ===\n");
    let path = default_aweti_path();
    let t_load = Instant::now();
    let g = load_grammar(&path);
    println!("load: {:?}", t_load.elapsed());
    println!(
        "entries={} mrules={} prules={} strata={} char_tables={}",
        g.entries.len(),
        g.mrules.len(),
        g.prules.len(),
        g.strata.len(),
        g.char_tables.len()
    );
    if g.char_tables.len() > 1 {
        println!(
            "NOTE: this grammar has >1 CharacterDefinitionTable. pg_foma::replace now resolves \
             each rule against its OWN owning stratum's table (fix-multitable-fst-compilation) -- \
             this probe still prints table[0]'s own segment count below for a quick sanity look, \
             not every table's."
        );
    }
    let table = &g.char_tables[0];
    println!("table[0] segment/boundary count: {}\n", table.len());
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();

    let mut rules_in_order: Vec<&PhonRuleDef> = Vec::new();
    for st in &g.strata {
        for &prid in &st.prules {
            rules_in_order.push(&g.prules[prid.0 as usize]);
        }
    }
    println!(
        "phonological rules in stratum order: {}\n",
        rules_in_order.len()
    );

    for pr in &rules_in_order {
        let PhonRuleDef::Rewrite(r) = pr else {
            println!("(metathesis rule, not attempted)");
            continue;
        };
        let t0 = Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            compile_rewrite_rule(&opts, &g, &alphabet, r)
        }));
        let elapsed = t0.elapsed();
        match result {
            Ok(Ok(Some((net, reports)))) => {
                println!(
                    "{} {:?}: COMPILED in {elapsed:?} -> {} states, {} arcs",
                    r.xml_id, r.name, net.statecount, net.arccount
                );
                for tr in &reports {
                    if tr.raw_product > 1 || tr.surviving > 1 {
                        println!(
                            "    alpha-tuple expansion: raw_product={} surviving={}",
                            tr.raw_product, tr.surviving
                        );
                    }
                }
            }
            Ok(Ok(None)) => println!(
                "{} {:?}: NOT COMPILED (unsupported construct) in {elapsed:?}",
                r.xml_id, r.name
            ),
            Ok(Err(budget_err)) => println!(
                "{} {:?}: COMPOSE BUDGET EXCEEDED: {budget_err} (in {elapsed:?})",
                r.xml_id, r.name
            ),
            Err(_) => println!(
                "{} {:?}: PANICKED during compile (in {elapsed:?})",
                r.xml_id, r.name
            ),
        }
    }

    println!("\n--- full cascade compile + compose ---");
    let t_all = Instant::now();
    let mut skipped: Vec<String> = Vec::new();
    let mut tuple_reports: Vec<(String, Vec<pg_foma::replace::TupleReport>)> = Vec::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compile_and_compose_rules(
            &opts,
            &g,
            &alphabet,
            &rules_in_order,
            &mut skipped,
            &mut tuple_reports,
        )
    }));
    let all_elapsed = t_all.elapsed();
    println!("cascade compile+compose: {all_elapsed:?}");
    match result {
        Ok(Ok(composed)) => {
            println!("skipped: {skipped:?}");
            match composed {
                Some(net) => println!(
                    "composed net: {} states, {} arcs",
                    net.statecount, net.arccount
                ),
                None => println!("composed net: NONE"),
            }
        }
        Ok(Err(budget_err)) => {
            println!("COMPOSE BUDGET EXCEEDED: {budget_err} (after {all_elapsed:?})")
        }
        Err(_) => println!("PANICKED during full-cascade compose (after {all_elapsed:?})"),
    }

    println!("\n=== done ===");
}
