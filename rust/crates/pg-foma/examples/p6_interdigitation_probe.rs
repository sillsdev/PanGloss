//! Interdigitation feasibility probe: do Amharic's phonological rules, including a 20-alpha-variable CV-merger, compile and compose via `pg_foma::replace`? Scope is compile + tuple-expansion + composition sizes only -- no recall gate and no `pg_foma::emit::emit()` call, since the underlying-form emitter doesn't yet support Amharic's `<AffixTemplate>` slots.

use std::path::{Path, PathBuf};
use std::time::Instant;

use foma::options::FomaOptions;

use pg_foma::replace::{
    compile_and_compose_rules, compile_rewrite_rule, is_fully_supported_shape, SegAlphabet,
};
use pg_grammar::model::{Grammar, PhonRuleDef};

const STACK_BYTES: usize = 512 * 1024 * 1024;

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn load_amharic() -> Grammar {
    let path = sample_path("amharic-hc.xml");
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load amharic-hc.xml: {e}"))
}

fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run)
        .expect("spawn large-stack worker thread");
    handle.join().expect("worker thread panicked");
}

fn run() {
    println!("=== P6 stretch goal: Amharic phonological rule compilation ===\n");
    let g = load_amharic();
    println!("char tables: {}", g.char_tables.len());
    let table = &g.char_tables[0];
    println!("table1 segment/boundary count: {}", table.len());
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

    // Per-rule compile attempt, isolated so one failure doesn't block measuring the others.
    for pr in &rules_in_order {
        let PhonRuleDef::Rewrite(r) = pr else {
            println!("{:?}: metathesis, not attempted", pr);
            continue;
        };
        let t0 = Instant::now();
        let result = compile_rewrite_rule(&opts, &g, r);
        let elapsed = t0.elapsed();
        match result {
            Some((net, reports)) => {
                println!(
                    "{} {:?} mode={:?} dir={:?} fully-supported-shape={}: COMPILED in {elapsed:?} \
                     -> {} states, {} arcs",
                    r.xml_id, r.name, r.mode, r.dir, is_fully_supported_shape(&g, r),
                    net.statecount, net.arccount
                );
                for tr in &reports {
                    if tr.raw_product > 1 || tr.surviving > 1 {
                        println!(
                            "    alpha-tuple expansion: raw_product={} surviving={} (var count could be dozens; \
                             tuple count is what matters, reports/08 §3.1)",
                            tr.raw_product, tr.surviving
                        );
                    }
                }
            }
            None => println!(
                "{} {:?}: NOT COMPILED (unsupported construct — see prototype report for which) in {elapsed:?}",
                r.xml_id, r.name
            ),
        }
    }

    // Full cascade compile+compose (stratum order), same call the Indonesian driver uses.
    println!("\n--- full cascade compile + compose ---");
    let t_all = Instant::now();
    let mut skipped: Vec<String> = Vec::new();
    let mut tuple_reports: Vec<(String, Vec<pg_foma::replace::TupleReport>)> = Vec::new();
    let composed = compile_and_compose_rules(
        &opts,
        &g,
        &alphabet,
        &rules_in_order,
        &mut skipped,
        &mut tuple_reports,
    );
    let all_elapsed = t_all.elapsed();
    println!("cascade compile+compose: {all_elapsed:?}");
    println!("skipped: {skipped:?}");
    match composed {
        Some(net) => println!(
            "composed net: {} states, {} arcs",
            net.statecount, net.arccount
        ),
        None => {
            println!("composed net: NONE (every rule was skipped or the grammar has zero prules)")
        }
    }

    println!("\n=== done ===");
}
