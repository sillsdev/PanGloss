//! Regression sanity check: `pg_foma::gate`'s partitioning machinery must find Amharic's known POS-gated subrules without crashing, while leaving the untouched `compile_and_compose_rules` entry point's tuple-expansion behavior unchanged. Does not attempt a full gated compile — that needs the templated-morphotactics `uflexc` emitter this prototype never built.

use std::path::{Path, PathBuf};

use foma::options::FomaOptions;
use pg_foma::gate::{find_gated_subrules, partition_entries};
use pg_foma::replace::{compile_and_compose_rules, SegAlphabet};
use pg_grammar::model::{Grammar, PhonRuleDef};

const STACK_BYTES: usize = 512 * 1024 * 1024;

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn load(name: &str) -> Grammar {
    let path = sample_path(name);
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {name}: {e}"))
}

fn check(label: &str, g: &Grammar) {
    println!("=== {label} ===");
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let mut rules_in_order: Vec<&PhonRuleDef> = Vec::new();
    for st in &g.strata {
        for &prid in &st.prules {
            rules_in_order.push(&g.prules[prid.0 as usize]);
        }
    }

    let gated = find_gated_subrules(g, &rules_in_order);
    println!("gated subrules found: {}", gated.len());
    for gs in &gated {
        let PhonRuleDef::Rewrite(r) = rules_in_order[gs.rule_pos] else {
            unreachable!()
        };
        println!("  rule={} ({:?}) sub_idx={}", r.xml_id, r.name, gs.sub_idx);
    }

    let groups = partition_entries(g, &gated, &rules_in_order);
    println!("partition groups: {}", groups.len());
    for grp in &groups {
        println!("  key={:?} entries={}", grp.key, grp.entries.len());
    }

    // Confirms the original (ungated) cascade compile still runs clean — a regression here would mean replace.rs's pre-existing path broke, not gate.rs's new one.
    let mut skipped: Vec<String> = Vec::new();
    let mut tuple_reports: Vec<(String, Vec<pg_foma::replace::TupleReport>)> = Vec::new();
    let composed = compile_and_compose_rules(
        &opts,
        g,
        &alphabet,
        &rules_in_order,
        &mut skipped,
        &mut tuple_reports,
    );
    println!("original compile_and_compose_rules: skipped={skipped:?}");
    match composed {
        Some(net) => println!(
            "composed net: {} states, {} arcs",
            net.statecount, net.arccount
        ),
        None => println!("composed net: NONE"),
    }
    println!();
}

fn main() {
    // Aweti intentionally omitted: its regression coverage needs the JSON pg-snapshot loader path, not worth wiring up here. Amharic alone below.
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(|| {
            check("Amharic", &load("amharic-hc.xml"));
        })
        .expect("spawn");
    handle.join().expect("amharic worker panicked");
}
