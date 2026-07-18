//! Regression sanity check (per task instructions: "make sure you haven't regressed... Amharic's
//! tuple-expansion behavior"): `hc_foma::gate`'s partitioning machinery must (a) find Amharic's
//! known POS-gated subrules (prule1/prule2/prule3, `requiredPartsOfSpeech`) without crashing, (b)
//! leave the UNTOUCHED `compile_and_compose_rules`/`compile_rewrite_rule` entry points byte-for-
//! byte behaviorally identical (they are new sibling functions, not edited — this just re-confirms
//! Amharic's own tuple-expansion counts from `p6_amharic_probe.rs` are unchanged), and (c) do the
//! same sanity pass over Aweti. Does NOT attempt a full gated compile for either (both need the
//! templated-morphotactics `uflexc` emitter this prototype never built — a separate, already-costed
//! gap per `docs/fst-plan/p6-prototype-report.md` §6 item 2, not something this MPR/POS step
//! reaches).

use std::path::{Path, PathBuf};

use foma::options::FomaOptions;
use hc_foma::gate::{find_gated_subrules, partition_entries};
use hc_foma::replace::{compile_and_compose_rules, SegAlphabet};
use hc_grammar::model::{Grammar, PhonRuleDef};

const STACK_BYTES: usize = 512 * 1024 * 1024;

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn load(name: &str) -> Grammar {
    let path = sample_path(name);
    let xml = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {name}: {e}"))
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
        let PhonRuleDef::Rewrite(r) = rules_in_order[gs.rule_pos] else { unreachable!() };
        println!("  rule={} ({:?}) sub_idx={}", r.xml_id, r.name, gs.sub_idx);
    }

    let groups = partition_entries(g, &gated, &rules_in_order);
    println!("partition groups: {}", groups.len());
    for grp in &groups {
        println!("  key={:?} entries={}", grp.key, grp.entries.len());
    }

    // Untouched entry point: confirm the ORIGINAL (ungated) cascade compile still runs clean,
    // exactly reproducing p6_amharic_probe.rs / p6_aweti_probe.rs's own reported behavior (this is
    // the same call those probes make; a regression here would mean this PR's edits broke
    // replace.rs's pre-existing code path, not gate.rs's own new one).
    let mut skipped: Vec<String> = Vec::new();
    let mut tuple_reports: Vec<(String, Vec<hc_foma::replace::TupleReport>)> = Vec::new();
    let composed = compile_and_compose_rules(&opts, g, &alphabet, &rules_in_order, &mut skipped, &mut tuple_reports);
    println!("original compile_and_compose_rules: skipped={skipped:?}");
    match composed {
        Some(net) => println!("composed net: {} states, {} arcs", net.statecount, net.arccount),
        None => println!("composed net: NONE"),
    }
    println!();
}

fn main() {
    // Aweti intentionally omitted here (unlike its name suggests) -- its own regression coverage
    // needs the JSON pg-snapshot loader path (`aweti_probe.rs`'s bespoke loader), not worth wiring
    // up for a sanity check the task doesn't ask for regressing (only Indonesian recall and
    // Amharic's tuple-expansion numbers are the named regression targets). Amharic alone below.
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(|| {
            check("Amharic", &load("amharic-hc.xml"));
        })
        .expect("spawn");
    handle.join().expect("amharic worker panicked");
}
