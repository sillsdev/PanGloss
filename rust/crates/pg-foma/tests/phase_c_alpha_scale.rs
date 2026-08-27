//! GATE: alpha-variable scale -- recall-parity, against an LHS+RHS identity rule that is
//! unambiguous by construction; see `pg_grammar_gen::build::alpha`'s module doc for why that
//! construction was chosen over two earlier ones that mismatched the real engine.

mod common;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::replace::{compile_and_compose_rules_with_budget, SegAlphabet};
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying_filtered_with_budget;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_grammar_gen::oracle::{sweep, OracleOpts};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

use common::gate_template::{assert_net_size_within, entry_id_of, recall_reachable};

fn recipe() -> Recipe {
    Recipe {
        name: "phase-c-alpha-scale",
        seed: 20260720,
        scale: ScaleKnobs {
            segment_inventory: 3,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 1,
            alpha_var_count: 2,
            alpha_class_size: 3,
            ..Default::default()
        },
    }
}

fn rules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
    g.strata[0]
        .prules
        .iter()
        .map(|&id| &g.prules[id.0 as usize])
        .collect()
}

#[test]
fn alpha_scale_recall_parity_via_generator_and_oracle() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated alpha-scale XML failed to load: {e}\n{}",
            rendered.xml
        )
    });

    let alpha = rendered
        .alpha
        .as_ref()
        .expect("recipe declared alpha_var_count > 0");
    assert_eq!(alpha.rule_xml_ids.len(), 2);
    assert_eq!(g.entries.len(), 1);
    assert_eq!(g.prules.len(), 2);

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro = rules_in_order(&g);
    let budget = ComposeBudget::with_caps(
        usize::MAX, usize::MAX);

    let root_id = entry_id_of(&g, &alpha.root_entry_xml_id);
    let mut entries = HashSet::new();
    entries.insert(root_id);
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(
        uemit.skipped.is_empty(),
        "no allomorph should be skipped: {:?}",
        uemit.skipped
    );
    let lexc_net = fsm_lexc_parse_string(&opts, None, &uemit.lexc_source)
        .unwrap_or_else(|| panic!("lexc must compile:\n{}", uemit.lexc_source));

    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let rules_net = compile_and_compose_rules_with_budget(
        &opts,
        &g,
        &alphabet,
        &ro,
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("alpha rule cascade must not hit any budget: {e}"));
    assert!(
        skipped.is_empty(),
        "neither alpha rule should be skipped: {skipped:?}"
    );
    let rules_net = rules_net.expect("the alpha rule cascade must compile to Some(net)");

    // Every alpha-bearing subrule's own tuple report: `alpha_class_size` (3) surviving assignments each, `var_count` (2) reports total.
    assert_eq!(
        tuple_reports.len(),
        2,
        "one tuple report per independent alpha rule: {tuple_reports:?}"
    );
    for (rule_xml_id, reports) in &tuple_reports {
        assert_eq!(
            reports.len(),
            1,
            "rule {rule_xml_id:?} must have exactly 1 alpha-bearing subrule"
        );
        assert_eq!(
            reports[0].surviving, 3,
            "rule {rule_xml_id:?} must have exactly alpha_class_size=3 surviving tuples"
        );
    }

    let composed = fsm_minimize(
        &opts,
        foma::constructions::fsm_compose(&opts, lexc_net, rules_net),
    );
    assert_net_size_within(&composed, 2_000, 20_000);

    // Oracle: bare-root generation for the single root runs the REAL phonological cascade (both alpha rules), ground truth for the surface form actually produced.
    let oracle_opts = OracleOpts {
        step_cap: 20_000,
        word_timeout: Some(Duration::from_millis(500)),
        max_rules_per_root: 0,
        max_total_words: 10,
    };
    let words = sweep(&g, &[root_id], &[], &oracle_opts);
    assert_eq!(
        words.len(),
        1,
        "bare-root generation must produce exactly 1 surface form, got {words:?}"
    );
    let surface = &words[0].surface;
    let width = tags::tag_width(g.morphemes.len());
    let tag = tags::root_tag_text(g.entries[root_id.0 as usize].morpheme, width);
    let encoded = alphabet
        .encode_query(surface)
        .unwrap_or_else(|| panic!("oracle surface {surface:?} must segment against table 0"));

    // Cross-check against `build::alpha`'s documented expectation (every rule is an identity map, so spelling must come back UNCHANGED) -- confirms this isn't accidentally a no-op grammar for some other reason.
    assert_eq!(*surface, alpha.root_shape, "oracle surface must equal the root's own (unchanged) spelling -- build::alpha's rules are identity maps");

    let t0 = Instant::now();
    let ok = recall_reachable(&composed, &encoded, &[tag]);
    let elapsed = t0.elapsed();
    assert!(
        ok,
        "100% recall required: oracle surface {surface:?} must be reachable in the composed net"
    );
    assert!(
        elapsed < Duration::from_millis(50),
        "per-word time {elapsed:?} exceeds the trip-wire"
    );
}
