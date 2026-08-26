//! Multi-table compiled-correctness gate: two tables with deliberately misaligned voice-feature indices, checking that rule compilation resolves each rule's own stratum table rather than defaulting to table 0.
//! Full argument and worked example: docs/research/pg-foma-replace-design-notes.md.

mod common;

use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::replace::{compile_and_compose_rules_with_budget, SegAlphabet};
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying_filtered_with_budget;
use pg_grammar::model::PhonRuleDef;
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};

use common::gate_template::{assert_net_size_within, entry_id_of, recall_reachable};

fn recipe() -> Recipe {
    Recipe {
        name: "phase-c-multi-table",
        seed: 20260720,
        scale: ScaleKnobs {
            entries_per_stratum: 2,
            segment_inventory: 2,
            ..ScaleKnobs::default()
        },
        construct: ConstructKnobs {
            table_count: 2,
            ..Default::default()
        },
    }
}

#[test]
fn multi_table_rewrite_compiles_correctly_against_its_owning_table() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated multi-table XML failed to load: {e}\n{}",
            rendered.xml
        )
    });

    assert_eq!(
        g.char_tables.len(),
        2,
        "recipe must produce exactly 2 tables"
    );
    assert_eq!(g.strata.len(), 2, "recipe must produce exactly 2 strata");
    assert_eq!(
        g.prules.len(),
        1,
        "recipe must produce exactly 1 phonological rule (the devoice demo)"
    );

    let table1 = &rendered.tables[1];
    let root_voiced = table1
        .roots
        .iter()
        .find(|r| r.voice_plus)
        .expect("table 1 must have a voice+ root");
    let root_voiceless = table1
        .roots
        .iter()
        .find(|r| !r.voice_plus)
        .expect("table 1 must have a voice- root");
    // Sanity, not the bug under test: a vacuous demonstration if the two roots share a spelling.
    assert_ne!(root_voiced.ch, root_voiceless.ch);

    let entry_voiced = entry_id_of(&g, &root_voiced.entry_xml_id);
    let entry_voiceless = entry_id_of(&g, &root_voiceless.entry_xml_id);

    let width = tags::tag_width(g.morphemes.len());
    let tag_voiced_root = tags::root_tag_text(g.entries[entry_voiced.0 as usize].morpheme, width);
    let tag_voiceless_root =
        tags::root_tag_text(g.entries[entry_voiceless.0 as usize].morpheme, width);

    let table1_chardef = &g.char_tables[1];
    let alphabet1 = SegAlphabet::new(table1_chardef);
    let opts = FomaOptions::default();
    // Unbounded caps: this gate is about the correct compile, not the budget mechanism.
    let budget = ComposeBudget::with_caps(
        usize::MAX, usize::MAX);

    let mut entries = std::collections::HashSet::new();
    entries.insert(entry_voiced);
    entries.insert(entry_voiceless);
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet1, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("stratum-1 lexc emission must not hit any budget: {e}"));
    assert!(
        uemit.skipped.is_empty(),
        "no allomorph should be skipped: {:?}",
        uemit.skipped
    );
    let lexc_net = fsm_lexc_parse_string(&opts, None, &uemit.lexc_source)
        .unwrap_or_else(|| panic!("stratum-1 lexc must compile:\n{}", uemit.lexc_source));

    let devoice_xml_id = rendered
        .devoice_rule_xml_id
        .clone()
        .expect("a table_count=2 recipe must produce a devoice demo rule");
    let devoice_rule = g
        .prules
        .iter()
        .find(|p| matches!(p, PhonRuleDef::Rewrite(r) if r.xml_id == devoice_xml_id))
        .expect("devoice demo rule must be present in g.prules");

    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let rule_net = compile_and_compose_rules_with_budget(
        &opts,
        &g,
        &alphabet1,
        &[devoice_rule],
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("devoice rule compile must not hit any budget: {e}"));
    // The fix changes WHICH table natural classes resolve against, not whether the rule compiles at all.
    assert!(
        skipped.is_empty(),
        "the devoice rule must compile (not be reported skipped): {skipped:?}"
    );
    let rule_net = rule_net.expect("the devoice rule must compile to Some(net), not skip to None");

    let composed = fsm_minimize(
        &opts,
        foma::constructions::fsm_compose(&opts, lexc_net, rule_net),
    );

    // Resource envelope: a two-entry, one-rule cascade must stay tiny.
    assert_net_size_within(&composed, 200, 2_000);

    let voiced_own_spelling = alphabet1
        .encode_query(&root_voiced.ch.to_string())
        .expect("voiced root's own character must segment against table 1");
    let voiceless_own_spelling = alphabet1
        .encode_query(&root_voiceless.ch.to_string())
        .expect("voiceless root's own character must segment against table 1");

    // --- Correctness 1: the root that SHOULD devoice does, and only that way. ---
    assert!(
        !recall_reachable(
            &composed,
            &voiced_own_spelling,
            std::slice::from_ref(&tag_voiced_root)
        ),
        "COMPILED-CORRECT assertion failed: the voice+ root's own (unrewritten) spelling is STILL \
         reachable as its surface form -- the obligatory devoice rewrite did not fire; either the \
         fix regressed, or this gate's own construction has drifted from its module doc"
    );
    assert!(
        recall_reachable(
            &composed,
            &voiceless_own_spelling,
            std::slice::from_ref(&tag_voiced_root)
        ),
        "the voice+ root's CORRECT devoiced form (the voiceless root's own spelling) is NOT \
         reachable -- the devoice rewrite is not resolving the right target segment"
    );

    // --- Correctness 2: the root that should stay unchanged is never spuriously rewritten. ---
    assert!(
        !recall_reachable(
            &composed,
            &voiced_own_spelling,
            std::slice::from_ref(&tag_voiceless_root)
        ),
        "the voice- root's spelling was WRONGLY rewritten to the voice+ root's own spelling -- \
         this is the exact cross-root mix-up the pre-fix bug produced"
    );
    assert!(
        recall_reachable(&composed, &voiceless_own_spelling, &[tag_voiceless_root]),
        "the voice- root's own (correct, unchanged) spelling is NOT reachable -- an already- \
         voiceless root must never lose its own surface form"
    );
}
