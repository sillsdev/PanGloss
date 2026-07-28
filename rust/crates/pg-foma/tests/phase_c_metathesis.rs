//! `openspec/changes/compile-fst-metathesis`: `PhonRuleDef::Metathesis` real FST semantics, via
//! [`pg_foma::replace::compile_metathesis_rule`]'s dedicated swap relation (that function's own
//! module doc: a per-branch literal cross-product union, mirroring `resolve_alpha_tuples`'s own
//! identity-preservation fix). BEFORE that change, every `<MetathesisRule>` was unconditionally
//! reported `skipped` (`"{xml_id} (metathesis, unhandled)"`, this file's OLD sole test). Now any
//! rule whose whole pattern is a shape `pg_foma::replace::pattern_slots` accepts (no `Quantifier`/
//! `Segments`/`Anchor`, no `Slot::Alpha`/`Slot::Repeat` anywhere) compiles to a real swap relation,
//! oracle-exact for the well-formed switch-tag convention (below) — for `Dir::LeftToRight`. As of
//! `openspec/changes/plan-construct-coverage-completion` task 4.6 (`docs/conformance/
//! needs-decision-resolutions.md` row 8), `Dir::RightToLeft` compiles too, via the SAME
//! mirror-and-reverse construction `compile_rtl_branch_net` already uses for RTL rewrite rules
//! (`pg_foma::replace`'s own module doc, "`Dir::RightToLeft`" section, has the full derivation) —
//! a proven SAFE SUPERSET of the true RTL relation (`ConfirmOnly`, never `Admit`), not proven
//! oracle-exact the way the `Dir::LeftToRight` case is.
//!
//! Synthetic, delanguaged fixtures (`openspec/changes/STAGING.md`'s "Hard rule: synthetic data
//! only"), named by construct. Each compilable fixture is checked against `pg_parse::Morpher`
//! (this codebase's own full-HC oracle), following `tests/phase_c_right_to_left.rs`/
//! `tests/two_table_symbol_divergence.rs`'s established methodology exactly
//! (`fst_candidate_set`/`oracle_candidate_set`, decode via `pg_foma::tags`).
//!
//! ## Scope this change compiles faithfully vs. leaves honestly unsupported
//! See `pg_foma::replace`'s own module doc (the "Metathesis" section, right above
//! `compile_metathesis_rule`) for the full, cited scope line. In short, EITHER `Dir` now: no
//! `Anchor` (`initialBoundaryCondition`/`finalBoundaryCondition`) anywhere in the pattern
//! (`metathesis_anchor_pattern_stays_honestly_unsupported`, below — not a metathesis-specific gap,
//! the identical refusal already applies to every `RewriteRuleDef` LHS/RHS/environment carrying
//! one); no `Quantifier`/`Segments`/disagree-polarity alpha var/`Slot::Alpha` anywhere. `Slot::Alpha`
//! is structurally IMPOSSIBLE for a `<MetathesisRule>` (not merely unattested — `pg_grammar::load::
//! load_metathesis_rule` resolves every node against an EMPTY `VarTable`, so an `<AlphaVariable>`
//! inside one errors the whole grammar load); a `Slot::Repeat` occurrence (a `<MetathesisRule>`'s
//! own `<PhoneticSequence>` is DTD-legal for `<OptionalSegmentSequence>` too) IS structurally
//! reachable, just never attested in any fixture this crate has authored, and stays refused for
//! either direction (`pg_foma::replace`'s own module doc has the full citation trail).
//!
//! ## `Dir::RightToLeft`: what changed (task 4.6) and what this file pins for it
//! `metathesis_right_to_left_reversal_matches_oracle_exactly` (below, RENAMED from
//! `metathesis_right_to_left_stays_honestly_unsupported` — that old name and behavior no longer
//! hold) is the load-bearing Stage-2 containment witness: every analysis `pg_parse::Morpher` finds
//! for its own words is a member of the FST proposer's candidate set. It reuses `RIGHT_TO_LEFT_XML`
//! (below) unchanged — that grammar has exactly ONE valid switch window per lexical entry, so it
//! cannot by itself distinguish `Dir::RightToLeft` from `Dir::LeftToRight` (see the empirical
//! finding two paragraphs down). `metathesis_right_to_left_differs_from_compiling_as_left_to_right`
//! (below) is the complementary, oracle-free witness that the CONSTRUCTION itself (not merely its
//! containment obligation) is genuinely direction-aware, mirroring `tests/
//! phase_c_right_to_left.rs`'s own "aa -> b" worked example.
//! `metathesis_right_to_left_switch_index_remap_matches_the_derived_formula` (below) is an
//! end-to-end behavioral confirmation of the remap `pg_foma::replace::
//! metathesis_mirror_switch_index_remap_tests` (in-crate) already pins arithmetically.
//!
//! **Empirical finding: `pg_rules::metathesis` is direction-blind, at least for the shape checked.**
//! A throwaway probe (deleted after recording this finding here) declared the SAME
//! two-adjacent-same-class-switch `MetathesisRule` under both `Dir::LeftToRight` and
//! `Dir::RightToLeft` and called `pg_rules::metathesis::synthesize` directly on an OVERLAPPING-
//! window input ("pqp", positions 0-1 and 1-2 both matching the switch pattern): both directions
//! synthesized identically ("qpp", the LEFTMOST window's swap) — `pg_rules::metathesis::
//! match_candidates` sorts candidates ascending (leftmost-first) REGARDLESS of `rule.dir`, and the
//! application loop always takes the first (i.e. leftmost) sorted candidate. This is the SAME
//! empirical shape `tests/phase_c_right_to_left.rs`'s own top doc found (BEFORE its own fix) for
//! ordinary `Iterative` rewrite rules — direction-blind pick-order, not direction-aware — and is
//! exactly why this file's containment witness reuses a NO-OVERLAP grammar (oracle recall for it is
//! byte-identical whichever direction is declared) while the DIFFERS-FROM-LTR witness is bare-
//! automaton and oracle-free (the one place an overlap genuinely needs to be constructed, and the
//! oracle's own behavior on it is irrelevant to what that witness checks).
//!
//! ## Two confirm-engine (`pg_rules::metathesis`) gaps found while building this containment suite
//! **UPDATE (2026-07-25): both gaps below are now FIXED** (a follow-on task, `pg_rules::
//! metathesis::build_analysis_pattern`'s own doc has the full citation trail + rationale for both
//! fixes). This section is kept, historically, as the record of what was found and why; the two
//! tests it names (`metathesis_grammar_gen_recipe_confirms_the_reversed_tag_round_trip` and
//! `metathesis_middle_context_node_now_matches_the_oracle`, renamed from their original
//! `_is_a_documented_oracle_gap` names) now assert the CORRECT, fixed behavior instead of pinning
//! the gap as data — the only change made to this file for that follow-on task, which owns
//! `pg_rules::metathesis` exclusively; this file's own `replace.rs`/`pg_foma` surface is untouched.
//!
//! `pg_rules::metathesis` was this change's oracle (design.md's own Ownership section: "the frozen
//! `MetathesisRule` switch identities and HermitCrab behavior"), and was READ-ONLY here (a bug fix
//! there was explicitly called out as a SEPARATE, out-of-scope task, design.md's own words — the
//! follow-on task referenced above). Two real, pre-existing gaps were discovered and were
//! documented (not silently worked around), per ADR 0001's recall-preserve discipline (the same
//! discipline `tests/phase_c_right_to_left.rs`'s own "Known, out-of-scope oracle gap" section
//! already established for RTL rewrites):
//!
//! 1. **Reversed switch-tag order** (originally
//!    `metathesis_reversed_tag_order_is_a_documented_oracle_gap`, now
//!    `metathesis_grammar_gen_recipe_confirms_the_reversed_tag_round_trip`, below):
//!    `pg_grammar::model::MetathesisRuleDef::left_switch`'s own doc claimed "After synthesis,
//!    whatever this index identifies always ends up FIRST in the output ... regardless of which one
//!    was physically first in `pattern.nodes`." Verified FALSE for the case where `left_switch`'s
//!    own node is physically FIRST and `right_switch`'s is physically LAST:
//!    `pg_rules::metathesis::synthesize`'s own `synthesis_reorder`/`move_nodes_after` algorithm is
//!    actually driven by PHYSICAL position (whichever switch is physically LAST always ends up
//!    FIRST in the output, tag-name-agnostic — direct hand-trace, confirmed by calling
//!    `pg_rules::metathesis::synthesize` directly: a rule tagging `leftSwitch` on the physically-
//!    FIRST node synthesizes `"qp"` to `"pq"`, not the vacuous no-op the doc's claim would predict).
//!    `build_analysis_pattern`'s own rebuild used to ALWAYS emit `left_switch`'s node first and
//!    `right_switch`'s node second, unconditionally — correct only when `left_switch` HAPPENS to be
//!    physically last already (the "well-formed" convention every real HermitCrab fixture this
//!    repo has ever seen actually uses, `machine/conformance/languages/metathesis-phase-isolation`'s
//!    `mrSimpleMeta`/`mrComplexMeta`). For the reversed tag order, synthesis and analysis used to
//!    disagree outright: `pg_parse::Morpher` found ZERO parses for EITHER the raw underlying
//!    spelling OR the correctly-synthesized swapped spelling. **Now fixed**: `build_analysis_
//!    pattern` orders by PHYSICAL position instead of tag name (identical output for every attested
//!    grammar, additionally correct for this reversed one), so analysis recovers the swapped
//!    spelling. `pg_grammar_gen::build::metathesis::build`'s own demo rule (used by the renamed test
//!    below) happens to use exactly this reversed convention -- this repo's own generator fixture
//!    was already a live witness of the gap, and is now a live witness of the fix.
//! 2. **Middle context node between the two switches** (originally
//!    `metathesis_middle_context_node_is_a_documented_oracle_gap`, now
//!    `metathesis_middle_context_node_now_matches_the_oracle`, below): `build_analysis_pattern`'s
//!    own doc used to say a context node strictly between the two switches "is dropped" from its
//!    rebuilt search pattern — but `synthesis_reorder` does NOT drop it ("a node strictly between
//!    them keeps its slot untouched"). A metathesis rule with >= 1 context node between its two
//!    switches (`machine/conformance/languages/metathesis-phase-isolation`'s own `mrComplexMeta`
//!    shape, minus its `finalBoundaryCondition` anchor) could therefore synthesize a real surface
//!    its OWN analysis side could never recognize (it searched for the two switches immediately
//!    adjacent, which the real surface never is). **Now fixed**: a middle node is preserved in the
//!    rebuild unless it resolves to a `CharDefKind::Boundary` (a boundary is excluded from the
//!    analysis match sequence regardless of pattern shape, so requiring one there could never be
//!    satisfied — see `build_analysis_pattern`'s own doc for why C#'s own unconditional drop never
//!    surfaced this as a problem for the one real shape, a `<BoundaryMarker>`, its own conformance
//!    suite ever exercises there).
//!
//! Both gaps were entirely inside `pg_rules::metathesis`, outside `replace.rs`'s single-owner
//! boundary. This change's OWN swap-relation construction was unaffected by either: it is
//! tag-name-agnostic (driven by physical position, matching `synthesis_reorder`'s REAL behavior,
//! not the doc's incorrect claim) and does not drop the middle node — so in BOTH gap cases the
//! FST's own proposal was already the semantically CORRECT one (verified directly against
//! `pg_rules::metathesis::synthesize`/`fst_candidate_set` below), and now the oracle confirms it too.

mod common;

use std::collections::HashSet;

use foma::apply::{apply_down, apply_init};
use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::reverse::fsm_reverse;

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::replace::{compile_and_compose_rules_with_budget, SegAlphabet};
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying_filtered_with_budget;
use pg_grammar::model::{Grammar, LexEntryId, PhonRuleDef};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};
use pg_parse::{Morpher, ParseOptions};

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

fn entry_id_of(g: &Grammar, xml_id: &str) -> LexEntryId {
    LexEntryId(
        g.entries
            .iter()
            .position(|e| g.morphemes[e.morpheme.0 as usize].xml_key == xml_id)
            .unwrap_or_else(|| panic!("no entry with xml id {xml_id:?}")) as u32,
    )
}

/// Every DECODED `apply_up` candidate for `query` against `net` -- the FST-proposer half of the
/// containment check (`tests/two_table_symbol_divergence.rs`'s own helper, reused verbatim).
fn fst_candidate_set(net: &foma::types::Fsm, query: &str) -> HashSet<(i32, Vec<u32>)> {
    let mut out = HashSet::new();
    let mut handle = apply_init(net);
    for s in handle.up(query) {
        let Some(path) = tags::decode_path(&s) else {
            continue;
        };
        for c in tags::to_candidates(&path) {
            out.insert((c.root_index, c.morphemes.iter().map(|m| m.0).collect()));
        }
    }
    out
}

/// The full-HC oracle's own candidate set for `surface`, restricted to `allowed_morphemes`
/// (`tests/two_table_symbol_divergence.rs`'s own helper, reused verbatim).
fn oracle_candidate_set(
    morpher: &Morpher,
    surface: &str,
    allowed_morphemes: &HashSet<u32>,
) -> HashSet<(i32, Vec<u32>)> {
    let outcome = morpher.parse_word_opts(surface, &ParseOptions::default());
    outcome
        .structured
        .iter()
        .filter(|a| a.morpheme_ids.iter().all(|m| allowed_morphemes.contains(m)))
        .map(|a| (a.root_morpheme_index, a.morpheme_ids.clone()))
        .collect()
}

/// Compiles `rule` (stratum 0's own table) via [`compile_and_compose_rules_with_budget`], composes
/// it after `lexc_source`, and minimizes -- the shared plumbing every containment witness below
/// uses (mirrors `tests/phase_c_right_to_left.rs`'s own `compile_net`).
fn compile_net(
    g: &Grammar,
    alphabet: &SegAlphabet,
    rule: &PhonRuleDef,
    lexc_source: &str,
) -> foma::types::Fsm {
    let opts = FomaOptions::default();
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let lexc_net = fsm_lexc_parse_string(&opts, None, lexc_source)
        .unwrap_or_else(|| panic!("lexc must compile:\n{lexc_source}"));
    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let rule_net = compile_and_compose_rules_with_budget(
        &opts,
        g,
        alphabet,
        &[rule],
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("rule compile must not hit any budget: {e}"))
    .expect("metathesis rule must now compile to Some(net)");
    assert!(skipped.is_empty(), "rule must not be skipped: {skipped:?}");
    fsm_minimize(&opts, fsm_compose(&opts, lexc_net, rule_net))
}

fn full_containment_check(
    g: &Grammar,
    entry_xml_id: &str,
    swapped_surface: &str,
    raw_surface: &str,
) {
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let entry = entry_id_of(g, entry_xml_id);
    let allowed: HashSet<u32> = [g.entries[entry.0 as usize].morpheme.0]
        .into_iter()
        .collect();

    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let entries: HashSet<LexEntryId> = [entry].into_iter().collect();
    let uemit = emit_underlying_filtered_with_budget(g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(g, usize::MAX);

    let query = alphabet
        .encode_query(swapped_surface)
        .unwrap_or_else(|| panic!("{swapped_surface:?} must segment against table 0"));
    let fst_out = fst_candidate_set(&net, &query);
    let oracle_out = oracle_candidate_set(&morpher, swapped_surface, &allowed);
    assert_eq!(
        oracle_out.len(),
        1,
        "oracle must recall {entry_xml_id} for its own obligatorily-metathesized surface \
         {swapped_surface:?}: {oracle_out:?}"
    );
    assert_eq!(
        fst_out, oracle_out,
        "CONTAINMENT: FST propose+decode must EQUAL the oracle for surface {swapped_surface:?}"
    );

    let oracle_raw = oracle_candidate_set(&morpher, raw_surface, &allowed);
    assert!(
        oracle_raw.is_empty(),
        "the raw un-swapped spelling {raw_surface:?} must have no oracle analysis: {oracle_raw:?}"
    );
}

// =================================================================================================
// metathesis-adjacent-singleton: two adjacent, distinct, singleton-class switch segments, the
// WELL-FORMED switch-tag convention every real HermitCrab fixture this repo has seen actually uses
// (`leftSwitch` tagging the node PHYSICALLY LAST, `rightSwitch` the one physically first --
// `machine/conformance/languages/metathesis-phase-isolation`'s own `mrSimpleMeta`). Proves EXACT
// oracle containment for the basic, attested shape.
// =================================================================================================

const ADJACENT_SINGLETON_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MetathesisAdjacentSingleton</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <!-- One distinct symbol value PER SEGMENT -- required so `pg_parse::Morpher`'s own
           analysis-side unapplication can disambiguate segments (`tests/phase_c_right_to_left.rs`'s
           own "one distinct symbol value per segment" note). -->
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols><Symbol id="symQ">q</Symbol><Symbol id="symP">p</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cQ"><Representations><Representation>q</Representation></Representations><FeatureValue feature="featId" symbolValues="symQ" /></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featId" symbolValues="symP" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="cQ" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="cP" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrAdjacent" leftSwitch="swP" rightSwitch="swQ">
        <Name>metathesisAdjacent</Name>
        <StructuralDescription>
          <PhoneticTemplate>
            <PhoneticSequence>
              <SimpleContext id="swQ" naturalClass="ncQ" />
              <SimpleContext id="swP" naturalClass="ncP" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrAdjacent">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryQP" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloQP"><PhoneticShape>qp</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>qp</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn metathesis_adjacent_singleton_swap_matches_oracle_exactly() {
    let g = load(ADJACENT_SINGLETON_XML);
    let metathesis_rules = g
        .prules
        .iter()
        .filter(|p| matches!(p, PhonRuleDef::Metathesis(_)))
        .count();
    assert_eq!(metathesis_rules, 1);
    full_containment_check(&g, "entryQP", "pq", "qp");
}

// =================================================================================================
// metathesis-multi-member-precision: BOTH switch positions are natural classes with more than one
// member ({q,r} / {s,t}) -- proves the per-branch literal cross-product union transposes EXACTLY
// the pair that matched, never any other combination a naive "[classA] [classB] -> [classB]
// [classA]" rendering would ALSO (incorrectly) accept (module doc on `compile_metathesis_rule`:
// "resolve every slot's own candidate members ... for each concrete assignment render ONE
// fully-literal branch"). ONE root ("q s") is enough to expose the bug: a naive nondeterministic
// cross-product would ALSO propose the root for "s r"/"t q"/"t r" (every OTHER combination of the
// two 2-member classes), none of which this rule's own single match ever produces.
// =================================================================================================

const MULTI_MEMBER_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MetathesisMultiMemberPrecision</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols>
          <Symbol id="symQ">q</Symbol><Symbol id="symR">r</Symbol>
          <Symbol id="symS">s</Symbol><Symbol id="symT">t</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cQ"><Representations><Representation>q</Representation></Representations><FeatureValue feature="featId" symbolValues="symQ" /></SegmentDefinition>
        <SegmentDefinition id="cR"><Representations><Representation>r</Representation></Representations><FeatureValue feature="featId" symbolValues="symR" /></SegmentDefinition>
        <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations><FeatureValue feature="featId" symbolValues="symS" /></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations><FeatureValue feature="featId" symbolValues="symT" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncSwitchA"><Name>SwitchA</Name><Segment segment="cQ" /><Segment segment="cR" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncSwitchB"><Name>SwitchB</Name><Segment segment="cS" /><Segment segment="cT" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrMultiMember" leftSwitch="swB" rightSwitch="swA">
        <Name>metathesisMultiMember</Name>
        <StructuralDescription>
          <PhoneticTemplate>
            <PhoneticSequence>
              <SimpleContext id="swA" naturalClass="ncSwitchA" />
              <SimpleContext id="swB" naturalClass="ncSwitchB" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrMultiMember">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryQS" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloQS"><PhoneticShape>qs</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>qs</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn metathesis_multi_member_classes_transpose_precisely_not_naively() {
    let g = load(MULTI_MEMBER_XML);
    let metathesis_rules = g
        .prules
        .iter()
        .filter(|p| matches!(p, PhonRuleDef::Metathesis(_)))
        .count();
    assert_eq!(metathesis_rules, 1);

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let entry_qs = entry_id_of(&g, "entryQS");
    let allowed: HashSet<u32> = [g.entries[entry_qs.0 as usize].morpheme.0]
        .into_iter()
        .collect();

    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let entries: HashSet<LexEntryId> = [entry_qs].into_iter().collect();
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(&g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(&g, usize::MAX);

    // "sq": the CORRECT, obligatory swap of underlying "qs".
    let query_sq = alphabet.encode_query("sq").expect("'sq' must segment");
    let fst_sq = fst_candidate_set(&net, &query_sq);
    let oracle_sq = oracle_candidate_set(&morpher, "sq", &allowed);
    assert_eq!(
        oracle_sq.len(),
        1,
        "oracle must recall entryQS for 'sq': {oracle_sq:?}"
    );
    assert_eq!(
        fst_sq, oracle_sq,
        "CONTAINMENT for 'sq' (the one genuine swap)"
    );

    // "qs": the raw, un-swapped spelling must never surface (obligatory metathesis).
    let oracle_raw = oracle_candidate_set(&morpher, "qs", &allowed);
    assert!(
        oracle_raw.is_empty(),
        "'qs' (obligatorily swapped) must have no oracle analysis"
    );

    // THE PRECISION WITNESS: "sr"/"tq"/"tr" are every OTHER combination of ncSwitchA={q,r} x
    // ncSwitchB={s,t} this rule's own pattern could also match against SOME OTHER root -- but
    // entryQS's own underlying is "qs", not "qr"/"rs"/"rt", so a FAITHFUL per-branch swap relation
    // must propose NOTHING for any of them. A naive "[q|r] [s|t] -> [s|t] [q|r]" rendering (the
    // bug this change's own cross-product fix avoids, module doc) would instead nondeterministically
    // accept ALL FOUR combinations for the SAME single match, wrongly proposing entryQS here too.
    for spurious in ["sr", "tq", "tr"] {
        let query = alphabet.encode_query(spurious).expect("must segment");
        let fst_spurious = fst_candidate_set(&net, &query);
        assert!(
            fst_spurious.is_empty(),
            "PRECISION: the FST must propose NOTHING for {spurious:?} -- a naive cross-product \
             swap would incorrectly accept it as an alternate transposition of the SAME rule, \
             cross-contaminating entryQS's own 'qs'/'sq' pair with the other class members: \
             {fst_spurious:?}"
        );
    }
}

// =================================================================================================
// metathesis-middle-context: the two switches are NOT adjacent -- one fixed, singleton "middle"
// context node sits between them (mirrors `machine/conformance/languages/metathesis-phase-
// isolation`'s own `mrComplexMeta` shape, minus its `finalBoundaryCondition` anchor -- see
// `metathesis_anchor_pattern_stays_honestly_unsupported`, below, for why that piece stays out of
// scope). UPDATE (2026-07-25): this file's own top doc, gap 2, is now FIXED
// (`pg_rules::metathesis::build_analysis_pattern` no longer drops a middle segment node) -- this is
// now a FULL containment witness (`full_containment_check`, same as the adjacent-singleton test
// above), not just an FST-only recall witness. Originally
// `metathesis_middle_context_node_is_a_documented_oracle_gap`; renamed to reflect the fix.
// =================================================================================================

const MIDDLE_CONTEXT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MetathesisMiddleContext</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols>
          <Symbol id="symQ">q</Symbol><Symbol id="symM">m</Symbol><Symbol id="symP">p</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cQ"><Representations><Representation>q</Representation></Representations><FeatureValue feature="featId" symbolValues="symQ" /></SegmentDefinition>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations><FeatureValue feature="featId" symbolValues="symM" /></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featId" symbolValues="symP" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="cQ" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncM"><Name>M</Name><Segment segment="cM" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="cP" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrMiddle" leftSwitch="swP" rightSwitch="swQ">
        <Name>metathesisMiddle</Name>
        <StructuralDescription>
          <PhoneticTemplate>
            <PhoneticSequence>
              <SimpleContext id="swQ" naturalClass="ncQ" />
              <SimpleContext naturalClass="ncM" />
              <SimpleContext id="swP" naturalClass="ncP" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrMiddle">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryQMP" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloQMP"><PhoneticShape>qmp</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>qmp</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn metathesis_middle_context_node_now_matches_the_oracle() {
    let g = load(MIDDLE_CONTEXT_XML);
    let metathesis_rules = g
        .prules
        .iter()
        .filter(|p| matches!(p, PhonRuleDef::Metathesis(_)))
        .count();
    assert_eq!(metathesis_rules, 1);
    // FIXED (was `oracle_out.is_empty()` -- this file's own top doc, gap 2): `pg_rules::
    // metathesis::build_analysis_pattern` no longer drops the middle context node, so the oracle
    // now recalls "pmq" (endpoints swapped, middle 'm' untouched) exactly like the FST does --
    // upgraded to the same full containment check the adjacent-singleton test above uses.
    full_containment_check(&g, "entryQMP", "pmq", "qmp");
}

// =================================================================================================
// metathesis-reversed-tag-order: `leftSwitch` tagging the node PHYSICALLY FIRST, `rightSwitch` the
// one physically last -- the OPPOSITE of the well-formed convention every real HermitCrab fixture
// this repo has seen uses (this file's own top doc, gap 1). `pg_grammar_gen`'s own
// `metathesis_rule_count` recipe (`pg_grammar_gen::build::metathesis::build`) happens to author
// exactly this convention -- reused here as the fix witness so the pre-existing recipe stays
// exercised, rather than only ever hitting the honest-skip gate it used to pin. UPDATE
// (2026-07-25): this file's own top doc, gap 1, is now FIXED (`pg_rules::metathesis::
// build_analysis_pattern` now orders by physical position, not tag name) -- this is now a full
// oracle-recall witness, not just an FST-only recall witness. Originally
// `metathesis_grammar_gen_recipe_reproduces_the_reversed_tag_oracle_gap`; renamed to reflect the
// fix.
// =================================================================================================

fn reversed_tag_recipe() -> Recipe {
    Recipe {
        name: "metathesis-reversed-tag-order",
        seed: 20260725,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            metathesis_rule_count: 1,
            ..Default::default()
        },
    }
}

#[test]
fn metathesis_grammar_gen_recipe_confirms_the_reversed_tag_round_trip() {
    let recipe = reversed_tag_recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = load(&rendered.xml);

    let metathesis = rendered
        .metathesis
        .as_ref()
        .expect("recipe declared metathesis_rule_count > 0");
    assert_eq!(metathesis.rule_xml_ids.len(), 1);
    assert_eq!(g.entries.len(), 1);
    let metathesis_rules = g
        .prules
        .iter()
        .filter(|p| matches!(p, PhonRuleDef::Metathesis(_)))
        .count();
    assert_eq!(metathesis_rules, 1);

    // This generator's own builder tags `leftSwitch` on the PHYSICALLY FIRST node
    // (`build::metathesis::build`'s own doc) -- the reversed convention this file's top doc names.
    let PhonRuleDef::Metathesis(rule) = &g.prules[0] else {
        panic!("expected a Metathesis-kind rule");
    };
    assert!(
        rule.left_switch < rule.right_switch,
        "this recipe's own reversed-tag convention"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let root_entry = &g.entries[0];
    let allowed: HashSet<u32> = [root_entry.morpheme.0].into_iter().collect();
    let underlying_text = root_entry.allomorphs[0].shape.text.clone();
    assert_eq!(underlying_text.chars().count(), 2);
    let swapped_text: String = underlying_text.chars().rev().collect();

    let shape = pg_grammar::segment::segment_phonemes_only(table, &underlying_text)
        .unwrap_or_else(|_| panic!("{underlying_text:?} must segment"));
    let synthesized = pg_rules::metathesis::synthesize(&g, rule, &shape);
    assert_eq!(synthesized.len(), 1, "the rule must apply obligatorily");
    let synthesized_text: String = synthesized[0]
        .interior()
        .map(|(_, _, cd, _)| {
            table
                .get(pg_grammar::chardef::CharDefId(cd))
                .representations()[0]
                .clone()
        })
        .collect();
    assert_eq!(
        synthesized_text, swapped_text,
        "pg_rules::metathesis::synthesize genuinely swaps (physical-position-driven, tag-name-\
         agnostic) -- NOT the vacuous no-op MetathesisRuleDef::left_switch's own doc comment would \
         predict for this reversed tag order (this file's top doc, gap 1)"
    );

    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let entries: HashSet<LexEntryId> = [LexEntryId(0)].into_iter().collect();
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(&g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(&g, usize::MAX);

    // RECALL (FST-only): the compiled relation must still PROPOSE the genuinely-synthesized
    // swapped surface, regardless of what the oracle's own (buggy, for this tag order) analysis
    // side can confirm.
    let query = alphabet
        .encode_query(&swapped_text)
        .unwrap_or_else(|| panic!("{swapped_text:?} must segment"));
    let fst_out = fst_candidate_set(&net, &query);
    assert!(
        !fst_out.is_empty(),
        "FST must propose the root for its own genuinely-synthesized surface {swapped_text:?}"
    );

    // FIXED (was `oracle_raw.is_empty() && oracle_swapped.is_empty()` -- this file's own top doc,
    // gap 1): `build_analysis_pattern` now orders by physical position, matching
    // `synthesis_reorder`'s real behavior, so the oracle now recalls the genuinely-synthesized
    // swapped spelling exactly (a full containment check, FST == oracle); the raw, un-swapped
    // spelling still has no oracle analysis (obligatory metathesis correctly still blocks it).
    let oracle_raw = oracle_candidate_set(&morpher, &underlying_text, &allowed);
    let oracle_swapped = oracle_candidate_set(&morpher, &swapped_text, &allowed);
    assert!(
        oracle_raw.is_empty(),
        "the raw un-swapped spelling {underlying_text:?} must have no oracle analysis \
         (obligatory metathesis): {oracle_raw:?}"
    );
    assert_eq!(
        oracle_swapped.len(),
        1,
        "oracle must recall the root for its own genuinely-synthesized surface {swapped_text:?}: \
         {oracle_swapped:?}"
    );
    assert_eq!(
        fst_out, oracle_swapped,
        "CONTAINMENT: FST propose+decode must EQUAL the oracle for surface {swapped_text:?}"
    );
}

// =================================================================================================
// metathesis-right-to-left: `Dir::RightToLeft` now compiles (`openspec/changes/
// plan-construct-coverage-completion` task 4.6; `docs/conformance/needs-decision-resolutions.md`
// row 8) via the SAME mirror-and-reverse construction `compile_rtl_branch_net` already uses for RTL
// rewrite rules -- see `pg_foma::replace`'s own module doc, "`Dir::RightToLeft`" section, for the
// full construction and the empirical finding (recorded there and in this file's own top doc) that
// `pg_rules::metathesis` is direction-BLIND, at least for the overlapping-window shape checked,
// mirroring what `tests/phase_c_right_to_left.rs`'s own top doc found (before its own fix) for
// ordinary rewrite rules.
// =================================================================================================

const RIGHT_TO_LEFT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MetathesisRightToLeft</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols><Symbol id="symQ">q</Symbol><Symbol id="symP">p</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cQ"><Representations><Representation>q</Representation></Representations><FeatureValue feature="featId" symbolValues="symQ" /></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featId" symbolValues="symP" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="cQ" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="cP" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrRtl" leftSwitch="swP" rightSwitch="swQ" multipleApplicationOrder="rightToLeftIterative">
        <Name>metathesisRtl</Name>
        <StructuralDescription>
          <PhoneticTemplate>
            <PhoneticSequence>
              <SimpleContext id="swQ" naturalClass="ncQ" />
              <SimpleContext id="swP" naturalClass="ncP" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrRtl">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryQP" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloQP"><PhoneticShape>qp</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>qp</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// RENAMED from `metathesis_right_to_left_stays_honestly_unsupported` (that old name/behavior no
/// longer hold -- `Dir::RightToLeft` now compiles, task 4.6). **This is the load-bearing Stage-2
/// containment witness**: every analysis `pg_parse::Morpher` finds for `entryQP`'s own words is a
/// member of `pg_foma::replace::compile_metathesis_rule`'s FST proposer candidate set -- turning
/// "the union is a superset, never an omission" from an argument into a checked claim, exactly the
/// same obligation `tests/phase_c_right_to_left.rs`'s own RTL containment witnesses discharge for
/// rewrite rules. `RIGHT_TO_LEFT_XML` has exactly ONE valid switch window (no overlap), so this
/// word alone cannot distinguish `Dir::RightToLeft` from `Dir::LeftToRight` -- see this file's own
/// top doc, "Empirical finding", for why that is a deliberate, honest choice, not an oversight:
/// this test's job is containment, not showing directional divergence (that is
/// `metathesis_right_to_left_differs_from_compiling_as_left_to_right`, below).
#[test]
fn metathesis_right_to_left_reversal_matches_oracle_exactly() {
    let g = load(RIGHT_TO_LEFT_XML);
    let PhonRuleDef::Metathesis(rule) = &g.prules[0] else {
        panic!("expected a Metathesis-kind rule");
    };
    assert_eq!(rule.dir, pg_grammar::model::Dir::RightToLeft);

    let metathesis_rules = g
        .prules
        .iter()
        .filter(|p| matches!(p, PhonRuleDef::Metathesis(_)))
        .count();
    assert_eq!(metathesis_rules, 1);

    // Same structural shape as `ADJACENT_SINGLETON_XML` above (leftSwitch tags the physically-last
    // node, the well-formed convention): underlying "qp" obligatorily metathesizes to "pq".
    full_containment_check(&g, "entryQP", "pq", "qp");
}

/// The complementary, oracle-free witness that the CONSTRUCTION itself is genuinely direction-
/// aware -- mirroring `tests/phase_c_right_to_left.rs`'s own "aa -> b" worked example exactly (bare
/// automaton, single-shot `apply_down`, no grammar/oracle involved at all). Needed because
/// `metathesis_right_to_left_reversal_matches_oracle_exactly` above deliberately uses a NO-OVERLAP
/// grammar (this file's top doc explains why) and so cannot, by itself, rule out the construction
/// silently degenerating to "compiled as if `Dir::LeftToRight`" -- without THIS test, that
/// regression would leave every other test in this file still green. See the test body's own
/// trailing comment for why this stays bare-automaton-only rather than also reproducing the proof
/// against a full grammar-level compile (tried, then deliberately removed).
///
/// # The bare-automaton proof
/// A metathesis switch pattern can never exhibit a genuine SAME-BRANCH overlap at width 2 (the
/// two switch positions would have to hold EQUAL values, making the swap a no-op) -- but a 4-node
/// pattern `[v0, v1, v2, v3]` with switches at `{0, 3}` and a period-2 assignment `[a, b, a, b]`
/// genuinely self-overlaps (shift 2) against the input `"ababab"`. The literal branch for this ONE
/// assignment is `"a b a b -> b b a a"` (module doc on `compile_metathesis_swap_net`: `rhs_vals`
/// with positions 0 and 3 transposed). Plain foma `->` prefers the LEFTMOST non-overlapping match
/// (window 0-3 first): `apply_down` on `"ababab"` gives `"bbaaab"`. The mirror rule for switches
/// `{0, 3}` in a 4-slot pattern remaps to `{n - 1 - 0, n - 1 - 3} = {3, 0}` -- the SAME set (this
/// specific placement's own `four_slot_outer_placement_is_its_own_mirror_set_...` unit test in
/// `pg_foma::replace` documents exactly this non-load-bearing coincidence) -- so the mirror pattern
/// is `reversed_slots([a,b,a,b]) = [b,a,b,a]`, and its own swap (positions 0,3 transposed) gives
/// mirror-RHS `[a,a,b,b]`, i.e. the branch `"b a b a -> a a b b"`. `fsm_reverse` of that compiled
/// branch, applied (single-shot `apply_down`) to the SAME `"ababab"`, gives `"abbbaa"` -- the
/// RIGHTMOST-preferring result (window 2-5 first) -- PROVABLY DIFFERENT from the plain branch's own
/// `"bbaaab"`, entirely independent of any oracle. (The FULL, all-paths `.down()` enumeration of
/// either branch already contains BOTH strings -- metathesis's own per-assignment full
/// literalization construction, module doc on `compile_metathesis_swap_net`, is nondeterministic
/// across valid tilings even before any reversal; what genuinely differs, and what the real FST
/// propose-then-confirm pipeline never even asks about, is single-shot `apply_down`'s own preferred
/// ordering -- the SAME distinction `tests/phase_c_right_to_left.rs`'s own worked example draws.)
#[test]
fn metathesis_right_to_left_differs_from_compiling_as_left_to_right() {
    let opts = FomaOptions::default();

    let plain = fsm_parse_regex(&opts, "a b a b -> b b a a", None, None).expect("plain compiles");
    let mut h = apply_init(&plain);
    assert_eq!(
        apply_down(&mut h, Some("ababab")),
        Some("bbaaab".to_string()),
        "plain LeftToRight-style compile must prefer the LEFTMOST non-overlapping match"
    );

    let mirror = fsm_parse_regex(&opts, "b a b a -> a a b b", None, None).expect("mirror compiles");
    let reversed = fsm_reverse(mirror);
    let mut h2 = apply_init(&reversed);
    assert_eq!(
        apply_down(&mut h2, Some("ababab")),
        Some("abbbaa".to_string()),
        "the reversal construction alone must prefer the RIGHTMOST non-overlapping match -- \
         PROVABLY DIFFERENT from the plain/LeftToRight-style branch above, entirely independent of \
         any oracle"
    );

    // Deliberately NOT extended to a full grammar-level `compile_and_compose_rules_with_budget`
    // comparison (tried while authoring this test, then removed): once the plain/reversed-mirror
    // branches above are unioned with the OTHER cross-product branches a real multi-position
    // natural-class pattern needs (module doc on `compile_metathesis_swap_net`'s own per-assignment
    // literalization -- most of those other branches are pure identity on any one specific probe
    // string), `apply_down`'s own single-shot exploration order over the LARGER unioned automaton
    // stopped reliably favoring the "abab"-literal branch's own transformation at all (empirically:
    // it found an IDENTITY path first for BOTH the real RTL compile and a `dir`-forced-LeftToRight
    // clone of the SAME rule, even with only one non-identity branch among four). That is an
    // artifact of `fsm_union`'s own state-numbering/exploration order, not evidence the underlying
    // MECHANISM stopped being direction-aware -- the bare two-branch proof above isolates that
    // mechanism directly and is the same scope `tests/phase_c_right_to_left.rs`'s own "aa -> b"
    // worked example uses for the identical claim (that test's own grammar-level portion instead
    // establishes ORACLE correctness, a claim this file's own empirical finding -- `pg_rules::
    // metathesis` is direction-blind, top doc -- makes inapplicable to metathesis).
}

/// Behavioral confirmation, end-to-end, of the switch-index remap
/// `pg_foma::replace::metathesis_mirror_switch_index_remap_tests` already pins arithmetically
/// in-crate: an ASYMMETRIC 5-node pattern (`Segment`s `a,b,c,d,e`, switches at indices 0 and 1,
/// three trailing fixed context nodes) has no natural-class alternation at all (every position is
/// a singleton `Segment`, cross product size 1), so its ENTIRE compiled relation is exactly one
/// literal mapping -- any remap error (an off-by-one landing on the WRONG pair of the mirror's own
/// slots, module doc on `metathesis_mirror_switch_indices`) would either panic (`Vec::swap` on an
/// out-of-bounds index) or silently produce a DIFFERENT literal output than the one derived by
/// hand below, so this test fails loudly either way under a regression.
#[test]
fn metathesis_right_to_left_switch_index_remap_matches_the_derived_formula() {
    // Pattern (document order): [a(swA=leftSwitch), b(swB=rightSwitch), c, d, e] -- switches at
    // indices 0,1, three trailing context nodes. Synthesis swaps positions 0,1: "a b c d e" ->
    // "b a c d e". Since there is only ONE possible assignment (every position a singleton
    // segment), Dir::RightToLeft's mirror-then-reverse branch must derive the SAME relation (no
    // overlap is possible with a single occurrence of the whole 5-node pattern) -- confirming the
    // remap lands on the CORRECT pair, not a shifted one that would swap some OTHER pair of
    // positions and so produce a visibly wrong output.
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MetathesisRtlRemapPin</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols>
          <Symbol id="symA">a</Symbol><Symbol id="symB">b</Symbol><Symbol id="symC">c</Symbol>
          <Symbol id="symD">d</Symbol><Symbol id="symE">e</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations><FeatureValue feature="featId" symbolValues="symA" /></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featId" symbolValues="symB" /></SegmentDefinition>
        <SegmentDefinition id="cC"><Representations><Representation>c</Representation></Representations><FeatureValue feature="featId" symbolValues="symC" /></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations><FeatureValue feature="featId" symbolValues="symD" /></SegmentDefinition>
        <SegmentDefinition id="cE"><Representations><Representation>e</Representation></Representations><FeatureValue feature="featId" symbolValues="symE" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrRemap" leftSwitch="swA" rightSwitch="swB" multipleApplicationOrder="rightToLeftIterative">
        <Name>metaRemap</Name>
        <StructuralDescription>
          <PhoneticTemplate>
            <PhoneticSequence>
              <Segment id="swA" segment="cA" />
              <Segment id="swB" segment="cB" />
              <Segment segment="cC" />
              <Segment segment="cD" />
              <Segment segment="cE" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrRemap">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryDummy" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloDummy"><PhoneticShape>abcde</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>dummy</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    let g = load(XML);
    let PhonRuleDef::Metathesis(rule) = &g.prules[0] else {
        panic!("expected a Metathesis-kind rule");
    };
    assert_eq!(rule.dir, pg_grammar::model::Dir::RightToLeft);
    assert_eq!((rule.left_switch, rule.right_switch), (0, 1));

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let net = compile_and_compose_rules_with_budget(
        &opts,
        &g,
        &alphabet,
        &[&g.prules[0]],
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("compile must not hit any budget: {e}"))
    .expect("RTL metathesis rule must compile to Some(net)");
    assert!(skipped.is_empty());

    // Single-shot AND full enumeration must both agree on exactly "bacde" (positions 0,1
    // transposed, 2-4 untouched) -- there is only one possible assignment, so there is no
    // leftmost/rightmost ambiguity for the remap to get wrong in a DIFFERENT way (that concern is
    // `metathesis_right_to_left_differs_from_compiling_as_left_to_right`'s own job); a wrong remap
    // here would swap some OTHER pair of the 5 positions and so produce a value other than "bacde".
    let query = alphabet
        .encode_query("abcde")
        .expect("'abcde' must segment");
    let mut h = apply_init(&net);
    let single = apply_down(&mut h, Some(&query));
    let expected = alphabet
        .encode_query("bacde")
        .expect("'bacde' must segment");
    assert_eq!(
        single,
        Some(expected.clone()),
        "the Dir::RightToLeft compile must swap positions 0,1 exactly (leaving 2,3,4 untouched) -- \
         an off-by-one remap would transpose a DIFFERENT pair and so miss this exact value"
    );
    let mut h2 = apply_init(&net);
    let all: Vec<String> = h2.down(&query).collect();
    assert!(
        all.iter().all(|s| *s == expected),
        "every path in the full relation must agree (no alternate-pair transposition sneaking in \
         from a wrong remap): {all:?}"
    );
}

// =================================================================================================
// metathesis-anchor: a `finalBoundaryCondition="true"` pattern (mirrors `metathesis-phase-
// isolation`'s own `mrComplexMeta` shape) stays honestly unsupported -- `pg_grammar::load::
// load_metathesis_rule` lowers the boundary condition to a `PatternNode::Anchor` node INSIDE
// `pattern.nodes`, and `pg_foma::replace::pattern_slots` refuses ANY `Anchor` occurrence
// grammar-wide today (not a metathesis-specific gap).
// =================================================================================================

const ANCHOR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MetathesisAnchor</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols><Symbol id="symQ">q</Symbol><Symbol id="symP">p</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cQ"><Representations><Representation>q</Representation></Representations><FeatureValue feature="featId" symbolValues="symQ" /></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featId" symbolValues="symP" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="cQ" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="cP" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrAnchor" leftSwitch="swP" rightSwitch="swQ">
        <Name>metathesisAnchor</Name>
        <StructuralDescription>
          <PhoneticTemplate finalBoundaryCondition="true">
            <PhoneticSequence>
              <SimpleContext id="swQ" naturalClass="ncQ" />
              <SimpleContext id="swP" naturalClass="ncP" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrAnchor">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryQP" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloQP"><PhoneticShape>qp</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>qp</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn metathesis_anchor_pattern_compiles_as_confirm_only_swap_superset() {
    let g = load(ANCHOR_XML);
    let PhonRuleDef::Metathesis(rule) = &g.prules[0] else {
        panic!("expected a Metathesis-kind rule");
    };
    assert!(
        matches!(
            rule.pattern.nodes.last(),
            Some(pg_grammar::model::PatternNode::Anchor(_))
        ),
        "finalBoundaryCondition must lower to a trailing PatternNode::Anchor"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let composed = compile_and_compose_rules_with_budget(
        &opts,
        &g,
        &alphabet,
        &[&g.prules[0]],
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("compile must not hit any budget: {e}"));

    let net = composed.expect("a final-anchor metathesis pattern must compile");
    assert!(skipped.is_empty(), "supported anchor must not be skipped: {skipped:?}");
    assert!(tuple_reports.is_empty());

    let query = alphabet.encode_query("qp").expect("underlying must segment");
    let expected = alphabet.encode_query("pq").expect("surface must segment");
    let mut h = apply_init(&net);
    let outputs = h.down(&query).collect::<Vec<_>>();
    assert!(
        outputs.iter().any(|s| s == &expected),
        "the swap must apply at the final word boundary: {outputs:?}"
    );
    // Edge anchors are erased only in the proposer. Complete confirmation applies the exact
    // boundary condition, so this is a safe ConfirmOnly over-approximation.
}
