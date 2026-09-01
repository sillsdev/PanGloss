//! Partition-k / MPR-POS subrule gating recall-parity: calls the production `pg_foma::gate::compile_gated_grammar` directly, generates each of the `2^k` bare-root entries, sweeps the real per-stratum cascade for ground truth, and verifies the compiled net relates the same surface string to the same root tag.

mod common;

use std::time::{Duration, Instant};

use foma::options::FomaOptions;

use pg_foma::gate::compile_gated_grammar;
use pg_foma::replace::SegAlphabet;
use pg_foma::tags;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_grammar_gen::oracle::{sweep, OracleOpts};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

use common::gate_template::{assert_net_size_within, entry_id_of, p99, recall_reachable};

fn recipe() -> Recipe {
    Recipe {
        name: "phase-c-partition-k",
        seed: 20260720,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            gated_subrule_count: 2,
            ..Default::default()
        },
    }
}

/// Stratum 0's `phonologicalRules` id-list, resolved to `&PhonRuleDef`s, document order.
fn rules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
    g.strata[0]
        .prules
        .iter()
        .map(|&id| &g.prules[id.0 as usize])
        .collect()
}

#[test]
fn partition_k_recall_parity_via_generator_and_oracle() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated partition-k XML failed to load: {e}\n{}",
            rendered.xml
        )
    });

    let gating = rendered
        .gating
        .as_ref()
        .expect("recipe declared gated_subrule_count > 0");
    assert_eq!(gating.rule_xml_ids.len(), 2);
    assert_eq!(
        gating.entry_xml_ids.len(),
        4,
        "2 independent gated subrules must realize 2^2 = 4 entries"
    );
    assert_eq!(g.entries.len(), 4);
    assert_eq!(g.prules.len(), 2);

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro = rules_in_order(&g);

    let result = compile_gated_grammar(&opts, &g, &alphabet, &ro)
        .unwrap_or_else(|e| panic!("gated compile must not hit any budget: {e}"));
    assert_eq!(
        result.groups, 4,
        "2 independent gated subrules must realize 2^2 = 4 groups"
    );
    // `result.skipped_rules` is NOT expected to be empty here: a gated rule not applicable to a given group is the ordinary/expected case, reported via the same path a genuinely-unsupported construct would use; the real correctness signal is the recall check below.
    println!(
        "partition-k: skipped_rules across all groups = {:?}",
        result.skipped_rules
    );
    let net = result.net.expect("gated compile must produce a network");

    // Resource envelope (design doc §4b): a 4-entry, 2-rule gated grammar must stay tiny.
    assert_net_size_within(&net, 500, 5_000);

    // Oracle (design doc §3): bare-root generation per entry -- module doc.
    let oracle_opts = OracleOpts {
        step_cap: 20_000,
        word_timeout: Some(Duration::from_millis(500)),
        max_rules_per_root: 0,
        max_total_words: 10,
    };
    let width = tags::tag_width(g.morphemes.len());

    let mut missed = Vec::new();
    let mut samples = Vec::new();
    for xml_id in &gating.entry_xml_ids {
        let entry_id = entry_id_of(&g, xml_id);
        let words = sweep(&g, &[entry_id], &[], &oracle_opts);
        assert_eq!(words.len(), 1, "bare-root generation must produce exactly 1 surface form for entry {xml_id:?}, got {words:?}");
        let surface = &words[0].surface;
        let tag = tags::root_tag_text(g.entries[entry_id.0 as usize].morpheme, width);
        let encoded = alphabet.encode_query(surface).unwrap_or_else(|| {
            panic!("entry {xml_id:?}'s oracle surface {surface:?} must segment against table 0")
        });

        let t0 = Instant::now();
        let ok = recall_reachable(&net, &encoded, &[tag]);
        samples.push(t0.elapsed());
        if !ok {
            missed.push((xml_id.clone(), surface.clone()));
        }
    }
    assert!(
        missed.is_empty(),
        "100% recall required on the 4 gating-key entries; missed: {missed:?}"
    );

    let per_word_p99 = p99(samples);
    assert!(
        per_word_p99 < Duration::from_millis(50),
        "per-word p99 {per_word_p99:?} exceeds the trip-wire"
    );
}
