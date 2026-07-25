//! `openspec/changes/compile-fst-metathesis`: `PhonRuleDef::Metathesis` real FST semantics, via
//! [`pg_foma::replace::compile_metathesis_rule`]'s dedicated swap relation (that function's own
//! module doc: a per-branch literal cross-product union, mirroring `resolve_alpha_tuples`'s own
//! identity-preservation fix). BEFORE this change, every `<MetathesisRule>` was unconditionally
//! reported `skipped` (`"{xml_id} (metathesis, unhandled)"`, this file's OLD sole test). Now a
//! `Dir::LeftToRight` rule whose whole pattern is a shape `pg_foma::replace::pattern_slots` accepts
//! (no `Quantifier`/`Segments`/`Anchor`, no `Slot::Alpha`/`Slot::Repeat` anywhere) compiles to a
//! real swap relation, oracle-exact for the well-formed switch-tag convention (below).
//!
//! Synthetic, delanguaged fixtures (`openspec/changes/STAGING.md`'s "Hard rule: synthetic data
//! only"), named by construct. Each compilable fixture is checked against `pg_parse::Morpher`
//! (this codebase's own full-HC oracle), following `tests/phase_c_right_to_left.rs`/
//! `tests/two_table_symbol_divergence.rs`'s established methodology exactly
//! (`fst_candidate_set`/`oracle_candidate_set`, decode via `pg_foma::tags`).
//!
//! ## Scope this change compiles faithfully vs. leaves honestly unsupported
//! See `pg_foma::replace`'s own module doc (the "Metathesis" section, right above
//! `compile_metathesis_rule`) for the full, cited scope line. In short: `Dir::LeftToRight` only
//! (`Dir::RightToLeft` is a documented, evidence-based scope boundary —
//! `metathesis_right_to_left_stays_honestly_unsupported`, below — deferred to a follow-on change
//! with its own oracle-matrix witness); no `Anchor` (`initialBoundaryCondition`/
//! `finalBoundaryCondition`) anywhere in the pattern
//! (`metathesis_anchor_pattern_stays_honestly_unsupported`, below — not a metathesis-specific gap,
//! the identical refusal already applies to every `RewriteRuleDef` LHS/RHS/environment carrying
//! one); no `Quantifier`/`Segments`/disagree-polarity alpha var/`Slot::Alpha` anywhere (not
//! attested in any `<MetathesisRule>` this crate has seen).
//!
//! ## Two confirm-engine (`pg_rules::metathesis`) gaps found while building this containment suite
//! `pg_rules::metathesis` is this change's oracle (design.md's own Ownership section: "the frozen
//! `MetathesisRule` switch identities and HermitCrab behavior"), and is READ-ONLY here (a bug fix
//! there is explicitly a SEPARATE, out-of-scope task, design.md's own words). Two real, pre-existing
//! gaps were discovered and are documented (not silently worked around), per ADR 0001's
//! recall-preserve discipline (the same discipline `tests/phase_c_right_to_left.rs`'s own "Known,
//! out-of-scope oracle gap" section already established for RTL rewrites):
//!
//! 1. **Reversed switch-tag order** (`metathesis_reversed_tag_order_is_a_documented_oracle_gap`,
//!    below): `pg_grammar::model::MetathesisRuleDef::left_switch`'s own doc claims "After
//!    synthesis, whatever this index identifies always ends up FIRST in the output ... regardless
//!    of which one was physically first in `pattern.nodes`." Verified FALSE for the case where
//!    `left_switch`'s own node is physically FIRST and `right_switch`'s is physically LAST:
//!    `pg_rules::metathesis::synthesize`'s own `synthesis_reorder`/`move_nodes_after` algorithm is
//!    actually driven by PHYSICAL position (whichever switch is physically LAST always ends up
//!    FIRST in the output, tag-name-agnostic — direct hand-trace, confirmed by calling
//!    `pg_rules::metathesis::synthesize` directly: a rule tagging `leftSwitch` on the physically-
//!    FIRST node synthesizes `"qp"` to `"pq"`, not the vacuous no-op the doc's claim would predict).
//!    `build_analysis_pattern`'s own rebuild, however, ALWAYS emits `left_switch`'s node first and
//!    `right_switch`'s node second (`nodes.push(pattern.nodes[left_switch]); nodes.push(pattern.
//!    nodes[right_switch]);`, unconditionally) — correct only when `left_switch` HAPPENS to be
//!    physically last already (the "well-formed" convention every real HermitCrab fixture this
//!    repo has ever seen actually uses, `machine/conformance/languages/metathesis-phase-isolation`'s
//!    `mrSimpleMeta`/`mrComplexMeta`). For the reversed tag order, synthesis and analysis disagree
//!    outright: `pg_parse::Morpher` finds ZERO parses for EITHER the raw underlying spelling OR the
//!    correctly-synthesized swapped spelling. `pg_grammar_gen::build::metathesis::build`'s own demo
//!    rule (used by `metathesis_grammar_gen_recipe_reproduces_the_reversed_tag_oracle_gap`, below)
//!    happens to use exactly this reversed convention -- this repo's own generator fixture is
//!    itself a live witness of the gap, not a bug in the generator.
//! 2. **Middle context node between the two switches**
//!    (`metathesis_middle_context_node_is_a_documented_oracle_gap`, below):
//!    `build_analysis_pattern`'s own doc says a context node strictly between the two switches "is
//!    dropped" from its rebuilt search pattern — but `synthesis_reorder` does NOT drop it ("a node
//!    strictly between them keeps its slot untouched"). A metathesis rule with >= 1 context node
//!    between its two switches (`machine/conformance/languages/metathesis-phase-isolation`'s own
//!    `mrComplexMeta` shape, minus its `finalBoundaryCondition` anchor) can therefore synthesize a
//!    real surface its OWN analysis side can never recognize (it searches for the two switches
//!    immediately adjacent, which the real surface never is).
//!
//! Both gaps are entirely inside `pg_rules::metathesis`, outside `replace.rs`'s single-owner
//! boundary. This change's OWN swap-relation construction is unaffected by either: it is
//! tag-name-agnostic (driven by physical position, matching `synthesis_reorder`'s REAL behavior,
//! not the doc's incorrect claim) and does not drop the middle node — so in BOTH gap cases the
//! FST's own proposal is the semantically CORRECT one (verified directly against
//! `pg_rules::metathesis::synthesize`/`fst_candidate_set` below); it is `pg_rules::metathesis`'s
//! analysis side that cannot yet confirm it. Per ADR 0001's recall-preserve discipline, the
//! compiler does NOT restrict itself to avoid these shapes -- it keeps proposing the semantically
//! correct swap, a safe (never under-proposing) superset relative to what today's oracle can
//! confirm, exactly like `tests/phase_c_right_to_left.rs`'s own RTL gap witness.

mod common;

use std::collections::HashSet;

use foma::apply::apply_init;
use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;

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
    let allowed: HashSet<u32> = [g.entries[entry.0 as usize].morpheme.0].into_iter().collect();

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
    let allowed: HashSet<u32> = [g.entries[entry_qs.0 as usize].morpheme.0].into_iter().collect();

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
    assert_eq!(oracle_sq.len(), 1, "oracle must recall entryQS for 'sq': {oracle_sq:?}");
    assert_eq!(fst_sq, oracle_sq, "CONTAINMENT for 'sq' (the one genuine swap)");

    // "qs": the raw, un-swapped spelling must never surface (obligatory metathesis).
    let oracle_raw = oracle_candidate_set(&morpher, "qs", &allowed);
    assert!(oracle_raw.is_empty(), "'qs' (obligatorily swapped) must have no oracle analysis");

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
// scope). This is a DOCUMENTED, oracle-independent witness (this file's own top doc, gap 2): the
// FST correctly proposes the swap with the middle untouched; `pg_rules::metathesis`'s own analysis
// side cannot confirm it (a pre-existing, out-of-scope confirm-engine gap).
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
fn metathesis_middle_context_node_is_a_documented_oracle_gap() {
    let g = load(MIDDLE_CONTEXT_XML);
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let entry_qmp = entry_id_of(&g, "entryQMP");
    let allowed: HashSet<u32> =
        [g.entries[entry_qmp.0 as usize].morpheme.0].into_iter().collect();

    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let entries: HashSet<LexEntryId> = [entry_qmp].into_iter().collect();
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(&g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(&g, usize::MAX);

    // RECALL (FST-only, oracle-independent): the FST must still PROPOSE "pmq" -- the endpoints
    // swap (q<->p), the middle 'm' keeps its own slot untouched -- proving the compiled relation
    // is semantically correct regardless of what the oracle can currently confirm.
    let query = alphabet.encode_query("pmq").expect("'pmq' must segment");
    let fst_out = fst_candidate_set(&net, &query);
    assert!(
        !fst_out.is_empty(),
        "FST must propose entryQMP for 'pmq' (the correct swap, middle untouched)"
    );

    // THE DOCUMENTED GAP (this file's own top doc, gap 2): `pg_rules::metathesis`'s own analysis
    // rebuild DROPS the middle node, so it can never recognize this correctly-synthesized surface.
    // Pinned as data, not silently avoided -- if a future fix to `pg_rules::metathesis` (a
    // separate, out-of-scope task per design.md's own Ownership section) makes this succeed, this
    // assertion failing is the expected, welcome signal to upgrade this test to a full containment
    // check.
    let oracle_out = oracle_candidate_set(&morpher, "pmq", &allowed);
    assert!(
        oracle_out.is_empty(),
        "KNOWN ORACLE GAP: pg_rules::metathesis's own analysis rebuild drops the middle context \
         node between the two switches, so it cannot recognize 'pmq' today -- see this file's top \
         doc, gap 2. If this ever starts finding a parse, revisit this test (not a regression in \
         replace.rs)."
    );

    // The raw, un-swapped spelling has no oracle analysis either (the rule's own SYNTHESIS pattern
    // still obligatorily matches "qmp", even though its ANALYSIS side can confirm neither form).
    let oracle_raw = oracle_candidate_set(&morpher, "qmp", &allowed);
    assert!(oracle_raw.is_empty(), "'qmp' must have no oracle analysis either");
}

// =================================================================================================
// metathesis-reversed-tag-order: `leftSwitch` tagging the node PHYSICALLY FIRST, `rightSwitch` the
// one physically last -- the OPPOSITE of the well-formed convention every real HermitCrab fixture
// this repo has seen uses (this file's own top doc, gap 1). `pg_grammar_gen`'s own
// `metathesis_rule_count` recipe (`pg_grammar_gen::build::metathesis::build`) happens to author
// exactly this convention -- reused here as the documented-gap witness so the pre-existing recipe
// stays exercised, rather than only ever hitting the honest-skip gate it used to pin.
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
fn metathesis_grammar_gen_recipe_reproduces_the_reversed_tag_oracle_gap() {
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
    assert!(rule.left_switch < rule.right_switch, "this recipe's own reversed-tag convention");

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
            table.get(pg_grammar::chardef::CharDefId(cd)).representations()[0].clone()
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

    // THE DOCUMENTED GAP (this file's own top doc, gap 1): the oracle finds NEITHER the raw
    // underlying spelling NOR the correctly-synthesized swapped spelling for this reversed tag
    // order -- `build_analysis_pattern`'s rebuild disagrees with `synthesis_reorder`'s real,
    // physical-position-driven behavior. Pinned as data, not silently avoided.
    let oracle_raw = oracle_candidate_set(&morpher, &underlying_text, &allowed);
    let oracle_swapped = oracle_candidate_set(&morpher, &swapped_text, &allowed);
    assert!(
        oracle_raw.is_empty() && oracle_swapped.is_empty(),
        "KNOWN ORACLE GAP: for the reversed switch-tag order, pg_rules::metathesis's synthesis and \
         analysis disagree, so NEITHER {underlying_text:?} nor {swapped_text:?} has an oracle \
         analysis today -- see this file's top doc, gap 1. If this ever starts finding a parse, \
         revisit this test (not a regression in replace.rs): raw={oracle_raw:?} \
         swapped={oracle_swapped:?}"
    );
}

// =================================================================================================
// metathesis-right-to-left: `Dir::RightToLeft` stays honestly unsupported (this change's own
// documented scope boundary -- `pg_foma::replace`'s module doc: the oracle is direction-AWARE for
// metathesis, unlike ordinary rewrite rules, so a safety-net union built on an RTL-rewrite-style
// direction-blindness assumption would be unsound here, not merely imprecise).
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

#[test]
fn metathesis_right_to_left_stays_honestly_unsupported() {
    let g = load(RIGHT_TO_LEFT_XML);
    let PhonRuleDef::Metathesis(rule) = &g.prules[0] else {
        panic!("expected a Metathesis-kind rule");
    };
    assert_eq!(rule.dir, pg_grammar::model::Dir::RightToLeft);

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

    assert!(
        composed.is_none(),
        "a Dir::RightToLeft metathesis rule must stay honestly unsupported, never a silent wrong \
         compile"
    );
    assert_eq!(skipped, vec!["mrRtl (metathesis, unhandled)".to_string()]);
    assert!(tuple_reports.is_empty());
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
fn metathesis_anchor_pattern_stays_honestly_unsupported() {
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

    assert!(
        composed.is_none(),
        "an Anchor-carrying metathesis pattern must stay honestly unsupported, never a silent \
         wrong compile"
    );
    assert_eq!(skipped, vec!["mrAnchor (metathesis, unhandled)".to_string()]);
    assert!(tuple_reports.is_empty());
}
