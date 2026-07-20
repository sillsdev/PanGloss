//! GATE 1 (`docs/fst-plan/phase-c-generator-design.md` §5/§6, priority (1)): multi-table
//! DETECT-WRONG gate.
//!
//! ## Initial gate mode: DETECT-WRONG, not recall-parity
//! `pg_foma::replace::table_of` hardcodes `&g.char_tables[0]` for EVERY natural-class resolution,
//! and `pg_foma::replace::SegAlphabet::token` is a pure function of a `CharDefId`'s raw numeric
//! index (`PUA_BASE + cd.0`) with no awareness of which table that index belongs to. Composing
//! those two facts (see `pg_grammar_gen::build::tables`'s module doc for the full derivation): a
//! phonological rule compiled for a stratum whose OWN table is NOT table 0 gets its natural-class
//! members resolved against table 0, then converted into tokens via the CALLER's (correctly
//! table-1-scoped) alphabet -- silently naming whatever segment happens to sit at that same
//! positional index in table 1, not the linguistically intended one.
//!
//! This recipe deliberately misaligns table 1's voice-feature-to-index assignment relative to
//! table 0's (`ConstructKnobs { table_count: 2, .. }` -> `build::tables::build`'s `misaligned =
//! true` path), so the mix-up is not just theoretically wrong but PROVABLY names the wrong
//! segment. The demo rule is an unconditional "devoice" rewrite (`ncVoicedAny -> ncVoicelessAny`,
//! no environment) on stratum 1 (table 1). Worked through concretely (2 segments per table: table
//! 0 = {voice+ at index 1, voice- at index 0}, table 1 = {voice+ at index 0, voice- at index 1}):
//! - `table_of` resolves `ncVoicedAny`/`ncVoicelessAny` against TABLE 0, yielding table-0-local
//!   `CharDefId(1)` (target) / `CharDefId(0)` (output).
//! - The caller's (correct) table-1 alphabet converts those SAME raw indices into table 1's own
//!   tokens: `CharDefId(1)` = table 1's voice- segment, `CharDefId(0)` = table 1's voice+ segment.
//! - So the COMPILED rule, in table 1's real token semantics, silently reads as "voice- ->
//!   voice+" -- the OPPOSITE of "devoice", targeting the WRONG segment.
//!
//! Two concretely wrong, deterministic, assertable behaviors follow, both checked below via the
//! shared compose-recall helper (`tests/common/gate_template.rs`):
//! 1. The root that SHOULD devoice (voice+) never changes -- its own unrewritten spelling is still
//!    (wrongly) accepted as its surface realization, and the CORRECT devoiced form is NOT
//!    reachable at all (genuine recall loss, not a harmless extra path).
//! 2. The root that should stay unchanged (voice-, already voiceless) gets WRONGLY rewritten to
//!    the other root's own spelling (a spurious change that should never happen).
//!
//! Per design doc §5, this gate's own module doc records that it FLIPS to a recall-parity mode
//! once the two hardcoded sites (`table_of` in `pg_foma::replace`, and the analogous
//! `resolve_alpha_tuples`) are fixed to thread the owning stratum's real table through -- at that
//! point every assertion below should INVERT (the "wrong" behaviors become unreachable, the
//! "correct" one becomes reachable), and this file's own assertions are what should fail first,
//! signalling the fix landed.

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
fn multi_table_wrongness_is_detected_and_precisely_characterized() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| panic!("generated multi-table XML failed to load: {e}\n{}", rendered.xml));

    assert_eq!(g.char_tables.len(), 2, "recipe must produce exactly 2 tables");
    assert_eq!(g.strata.len(), 2, "recipe must produce exactly 2 strata");
    assert_eq!(g.prules.len(), 1, "recipe must produce exactly 1 phonological rule (the devoice demo)");

    let table1 = &rendered.tables[1];
    let root_voiced = table1.roots.iter().find(|r| r.voice_plus).expect("table 1 must have a voice+ root");
    let root_voiceless = table1.roots.iter().find(|r| !r.voice_plus).expect("table 1 must have a voice- root");
    // Sanity on the recipe's own shape (not the bug): the two roots must be spelled differently,
    // or the whole demonstration is vacuous.
    assert_ne!(root_voiced.ch, root_voiceless.ch);

    let entry_voiced = entry_id_of(&g, &root_voiced.entry_xml_id);
    let entry_voiceless = entry_id_of(&g, &root_voiceless.entry_xml_id);

    let width = tags::tag_width(g.morphemes.len());
    let tag_voiced_root = tags::root_tag_text(g.entries[entry_voiced.0 as usize].morpheme, width);
    let tag_voiceless_root = tags::root_tag_text(g.entries[entry_voiceless.0 as usize].morpheme, width);

    let table1_chardef = &g.char_tables[1];
    let alphabet1 = SegAlphabet::new(table1_chardef);
    let opts = FomaOptions::default();
    // Never usize::MAX / no artificial caps: this gate is about DETECTING the wrongness, not
    // about the budget mechanism itself (that's `tests/common`'s honest-failure helper's job,
    // exercised by GATE 2's own over-budget-shaped assertions if/when stage 2 adds them here).
    let budget = ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None);

    let mut entries = std::collections::HashSet::new();
    entries.insert(entry_voiced);
    entries.insert(entry_voiceless);
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet1, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("stratum-1 lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty(), "no allomorph should be skipped: {:?}", uemit.skipped);
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
    let rule_net = compile_and_compose_rules_with_budget(&opts, &g, &alphabet1, &[devoice_rule], &mut skipped, &mut tuple_reports, &budget)
        .unwrap_or_else(|e| panic!("devoice rule compile must not hit any budget: {e}"));
    // Detect-wrong's own "compile succeeds" half (design doc §5): the rule is NOT reported
    // skipped, even though (as the assertions below show) its compiled effect is wrong. A
    // production caller gets no signal at all that anything went wrong here.
    assert!(skipped.is_empty(), "the devoice rule must compile (not be reported skipped): {skipped:?}");
    let rule_net = rule_net.expect("the devoice rule must compile to Some(net), not skip to None");

    let composed = fsm_minimize(&opts, foma::constructions::fsm_compose(&opts, lexc_net, rule_net));

    // Resource envelope (design doc §4b): this is a two-entry, one-rule cascade -- must stay tiny.
    assert_net_size_within(&composed, 200, 2_000);

    let voiced_own_spelling = alphabet1
        .encode_query(&root_voiced.ch.to_string())
        .expect("voiced root's own character must segment against table 1");
    let voiceless_own_spelling = alphabet1
        .encode_query(&root_voiceless.ch.to_string())
        .expect("voiceless root's own character must segment against table 1");

    // --- Wrongness 1: the root that SHOULD devoice never does. ---
    assert!(
        recall_reachable(&composed, &voiced_own_spelling, &[tag_voiced_root.clone()]),
        "DETECT-WRONG assertion failed to reproduce: the voice+ root's own (unrewritten) spelling \
         is no longer reachable as its surface form -- either the bug stopped reproducing, or this \
         gate's own construction has drifted from the derivation in its module doc"
    );
    assert!(
        !recall_reachable(&composed, &voiceless_own_spelling, &[tag_voiced_root.clone()]),
        "the voice+ root's CORRECT devoiced form (the voiceless root's own spelling) is reachable -- \
         this would mean the rule fired correctly after all, contradicting this gate's own module doc"
    );

    // --- Wrongness 2: the root that should stay unchanged gets spuriously rewritten. ---
    assert!(
        recall_reachable(&composed, &voiced_own_spelling, &[tag_voiceless_root.clone()]),
        "DETECT-WRONG assertion failed to reproduce: the voice- root's spelling never gets \
         (wrongly) rewritten to the voice+ root's own spelling -- either the bug stopped \
         reproducing, or this gate's own construction has drifted from its module doc"
    );
    // The buggy rule's compiled target is unconditional (obligatory, no environment) and happens
    // to BE the voice- root's own segment (module doc's worked-through derivation) -- so, unlike
    // an environment-gated rewrite, there is no "elsewhere identity" survivor here: the voice-
    // root's ORIGINAL spelling is not merely also-reachable alongside the wrong one, it is GONE
    // entirely. Verified empirically against the actual compiled net (not assumed): this is a
    // stronger, but still honestly-characterized, form of the same wrongness -- a full obligatory
    // mis-rewrite, not a partial one.
    assert!(
        !recall_reachable(&composed, &voiceless_own_spelling, &[tag_voiceless_root]),
        "the voice- root's own (correct, unchanged) spelling is unexpectedly STILL reachable -- \
         the compiled rule is no longer an unconditional obligatory rewrite of this segment, \
         contradicting this gate's own verified derivation"
    );
}
