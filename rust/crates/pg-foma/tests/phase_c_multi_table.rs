//! GATE 1: the multi-table COMPILED-CORRECT gate.
//!
//! ## Why table ownership must be threaded explicitly, not defaulted
//! `pg_foma::replace::SegAlphabet::token` is a pure function of a `CharDefId`'s raw numeric index
//! (`PUA_BASE + cd.0`) with no awareness of which table that index belongs to (see
//! `pg_grammar_gen::build::tables`'s module doc for the full derivation), so any natural-class
//! resolution that silently defaults to table 0 rather than the rule's own owning stratum's table
//! will name whatever segment happens to sit at that same positional index in a DIFFERENT table --
//! not the linguistically intended one. That failure mode is real and concretely observable: a
//! phonological rule compiled for a stratum whose OWN table was NOT table 0, resolved against
//! table 0 and then converted into tokens via the CALLER's (correctly table-1-scoped) alphabet,
//! produces two deterministic wrong behaviors: a voice+ root that never devoices, and a voice-
//! root spuriously rewritten to the voice+ root's own spelling.
//!
//! [`pg_foma::replace::pattern_slots`]/[`pg_foma::replace::resolve_alpha_tuples`] take an explicit
//! `&CharDefTable` parameter, and [`pg_foma::replace::compile_rewrite_rule_subset`] resolves it
//! ONCE per rule via `pg_foma::replace::owning_table` (the rule's OWN stratum's
//! `StratumDef::table` -- never an implicit table-zero default). Every assertion below checks that
//! this design produces the linguistically-correct result.
//!
//! This recipe deliberately misaligns table 1's voice-feature-to-index assignment relative to
//! table 0's (`ConstructKnobs { table_count: 2, .. }` -> `build::tables::build`'s `misaligned =
//! true` path), so a correct compile is not merely "coincidentally didn't break" but PROVABLY
//! resolves the linguistically-intended segment despite the two tables disagreeing about which
//! raw index means which voice value. The demo rule is an unconditional "devoice" rewrite
//! (`ncVoicedAny -> ncVoicelessAny`, no environment) on stratum 1 (table 1). Worked through
//! concretely (2 segments per table: table 0 = {voice+ at index 1, voice- at index 0}, table 1 =
//! {voice+ at index 0, voice- at index 1}):
//! - `owning_table` resolves `rule`'s owning stratum (stratum 1) and returns table 1 -- so
//!   `ncVoicedAny`/`ncVoicelessAny` are now resolved against TABLE 1 ITSELF, yielding table-1-local
//!   `CharDefId`s that name table 1's OWN voice+/voice- segments directly, not table 0's.
//! - The SAME alphabet (also built from table 1) converts those table-1-local ids into table 1's
//!   own tokens -- consistent, single-table-per-rule token space, no cross-table reinterpretation.
//! - So the COMPILED rule, in table 1's real token semantics, correctly reads as "voice+ ->
//!   voice-" -- true devoicing, targeting the RIGHT segment.
//!
//! Two concretely correct, deterministic, assertable behaviors follow, both checked below via the
//! shared compose-recall helper (`tests/common/gate_template.rs`):
//! 1. The root that SHOULD devoice (voice+) has its own unrewritten spelling become UNREACHABLE as
//!    its surface form (the obligatory rewrite fires), and its CORRECT devoiced form (the voiceless
//!    root's own spelling) IS reachable.
//! 2. The root that should stay unchanged (voice-, already voiceless) keeps its own spelling
//!    reachable, and is never spuriously rewritten to the OTHER root's spelling.
//!
//! Single-table grammars are BYTE IDENTICAL under this fix: every stratum's `table: TableId` is 0
//! in a single-table grammar, so `owning_table` always resolves to `g.char_tables[0]` -- the exact
//! value the old hardcoded default returned. `tests/p6_gate_parity.rs`'s byte-exact Amharic
//! state/arc-count regression guard (single-table) staying green is that invariant's own proof.

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
    // Sanity on the recipe's own shape (not the bug): the two roots must be spelled differently,
    // or the whole demonstration is vacuous.
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
    // Never usize::MAX / no artificial caps: this gate is about the CORRECT compile, not about
    // the budget mechanism itself.
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );

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
    // The rule must still compile (not be reported skipped) -- the fix changes WHICH table its
    // natural classes resolve against, not whether the rule is a supported shape at all.
    assert!(
        skipped.is_empty(),
        "the devoice rule must compile (not be reported skipped): {skipped:?}"
    );
    let rule_net = rule_net.expect("the devoice rule must compile to Some(net), not skip to None");

    let composed = fsm_minimize(
        &opts,
        foma::constructions::fsm_compose(&opts, lexc_net, rule_net),
    );

    // Resource envelope (design doc §4b): this is a two-entry, one-rule cascade -- must stay tiny.
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
