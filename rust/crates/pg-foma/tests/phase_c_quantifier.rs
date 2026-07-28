//! GATE (`docs/fst-plan/phase-c-generator-design.md` §6, priority (7)): quantifier /
//! `OptionalSegmentSequence` compile gate -- pure test-writing, this pins the loader/compiler's
//! ACTUAL disposition per shape, so a regression (a genuinely out-of-scope shape silently
//! mis-compiling, or a now-supported shape silently regressing back to a bail) is caught.
//!
//! `pg_foma::replace::pattern_slots` returns `None` on any out-of-scope `PatternNode::Quantifier`
//! it meets in a REWRITE rule's own LHS/RHS/environment (inverted-finite/over-budget-finite/
//! alpha-nested), which `compile_rewrite_rule_subset` turns into `Ok(None)` for the whole rule;
//! `compile_and_compose_rules_with_budget` reports that via `skipped.push(rule.xml_id.clone())`
//! (design doc §5's "Honest skip now" list).
//!
//! ## Bounded quantifiers now compile (`openspec/changes/compile-bounded-fst-quantifiers`)
//! A FINITELY bounded, alpha-free quantifier (`min`/`max` both concrete) compiles now, via
//! `pg_foma::replace::Slot::Repeat` (that variant's own doc: foma's native `^{min,max}` bounded-
//! repetition xre operator). `quantifier_bounded_environment_compiles_and_matches_oracle` (below) is
//! this change's own containment fixture: a bounded quantifier used INSIDE a rule's right
//! environment, proposer-to-confirm CONTAINMENT-checked against `pg_parse::Morpher` (this codebase's
//! own full-HC oracle) at BOTH its `min` and `max` boundary occurrence counts, plus a negative
//! control below `min`.
//!
//! ## Genuinely unbounded quantifiers ALSO now compile (`openspec/changes/
//! build-unbounded-quantifier-support`, tasks.md 4.5)
//! The construct's own ORIGINAL, unbounded (`max="-1"`) shape used to be this file's own
//! honest-skip witness (`quantifier_rule_is_honestly_reported_skipped`, the generator-produced
//! LHS-focus fixture, and `quantifier_unbounded_environment_stays_honestly_unsupported`, the
//! right-environment fixture) -- `pg_foma::replace::Slot::Repeat`'s `max: Option<u32>` widening now
//! compiles this shape too (foma's native `*`/`^>N` unbounded-repetition xre operator instead of
//! `^{min,max}`), so BOTH witnesses below are renamed and flipped to prove the NEW disposition
//! (compiles, no longer skipped) instead of the old one. `MAX_QUANTIFIER_BOUND`/the inverted-bound
//! check still apply ONLY to a FINITE `max` -- an inverted-finite or over-budget-finite or
//! alpha-nested quantifier stays exactly as unsupported as before (unaffected by this widening).
//!
//! **Why the environment, not the LHS/RHS focus** (a documented, load-bearing choice, not an
//! arbitrary one): `pg_rules::rewrite::width_matches`'s own doc names a "Shared width-mismatch
//! guard" that requires a matched span's PHYSICAL width to equal the rule's raw `lhs.nodes.len()`/
//! `rhs.nodes.len()` -- a plain node COUNT that is always exactly 1 for "one `Quantifier` node
//! occupies the whole LHS/RHS", regardless of how many physical segments it actually consumes. A
//! Quantifier match whose real width differs from that fixed count (any `max > 1`, or a `min == 0`
//! zero-occurrence skip) is silently discarded by this guard before the RHS is ever applied --
//! independent of this change (the guard predates it; its own doc explains it exists for an
//! unrelated scenario that merely also catches this one). This is a real, pre-existing,
//! now-surfaced confirm-engine gap, documented in `pg_foma::replace`'s own module doc ("Confirm-
//! engine finding") rather than silently worked around -- exactly the RTL precedent's own
//! "recall-preserve, don't paper over" discipline (`tests/phase_c_right_to_left.rs`'s "Known,
//! out-of-scope oracle gap" section). A `Quantifier` used INSIDE an environment has NO such gap:
//! `pg_rules::rewrite::left_env_match`/`right_env_match` test only first-match EXISTENCE
//! (`Transduce::first_match`) against a `PatternBridge::compile_pattern`-compiled (Quantifier-
//! faithful) environment FST, never a positional per-node array -- no width count to mismatch. This
//! file's own containment fixture therefore places its quantifier there, where exact oracle
//! equality is provable today, following the SAME `fst_candidate_set`/`oracle_candidate_set`
//! methodology `tests/phase_c_right_to_left.rs`/`tests/two_table_symbol_divergence.rs` already use.

mod common;

use std::collections::HashSet;

use foma::apply::{apply_down, apply_init};
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

fn recipe() -> Recipe {
    Recipe {
        name: "phase-c-quantifier",
        seed: 20260720,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            quantifier_bound: Some((1, 3)),
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

/// **Was honestly skipped; now compiles** (`openspec/changes/build-unbounded-quantifier-support`):
/// the generator's own `build::quantifier` builder (`pg-grammar-gen/src/build/quantifier.rs`)
/// always mints a genuinely UNBOUNDED (`max="-1"`) quantifier occupying the rule's WHOLE LHS focus
/// (`<PhoneticInput>`) -- `pg_foma::replace::Slot::Repeat`'s `max: Option<u32>` widening now renders
/// this via foma's native `E^>N`/`E*` operator (`crate::lower::render_slots`'s own doc), so this
/// rule compiles to `Some(net)`, no longer `skipped`. (This rule's own FULL-RECALL containment
/// against `pg_rules::rewrite` is a separate, documented, pre-existing confirm-engine gap for ANY
/// LHS/RHS-focus-quantified rule regardless of occurrence-count shape -- `crate::replace` module
/// doc's "Confirm-engine finding" -- this unit test only exercises the FST COMPILE side, same as
/// this file's other two witnesses.)
#[test]
fn quantifier_unbounded_lhs_focus_now_compiles() {
    let recipe = recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated quantifier XML failed to load: {e}\n{}",
            rendered.xml
        )
    });

    let quantifier = rendered
        .quantifier
        .as_ref()
        .expect("recipe declared quantifier_bound.is_some()");
    assert_eq!(g.prules.len(), 1);
    assert_eq!(g.entries.len(), 1);

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro = rules_in_order(&g);
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
        &ro,
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("compile must not hit any budget: {e}"));

    // Post-`build-unbounded-quantifier-support`: the quantifier-bearing rule is no longer skipped
    // at all -- it compiles to a real network, alpha-free (a trivial 1-entry tuple report).
    assert!(
        skipped.is_empty(),
        "the unbounded LHS-focus quantifier rule must no longer be skipped: {skipped:?}"
    );
    assert!(
        composed.is_some(),
        "an unbounded LHS-focus quantifier must compile to a real network, not a no-op cascade"
    );
    assert_eq!(
        tuple_reports.len(),
        1,
        "exactly one compiled (alpha-free) rule contributes a trivial tuple report"
    );
    assert_eq!(tuple_reports[0].0, quantifier.rule_xml_id);
    assert_eq!(tuple_reports[0].1.len(), 1, "one alpha-free subrule");
    assert_eq!(tuple_reports[0].1[0].raw_product, 1);
    assert_eq!(tuple_reports[0].1[0].surviving, 1);
}

// =================================================================================================
// Bounded quantifier, IN AN ENVIRONMENT (`openspec/changes/compile-bounded-fst-quantifiers`; this
// file's own top doc, "Bounded quantifiers now compile" / "Why the environment, not the LHS/RHS
// focus"): synthetic, delanguaged fixtures, named by construct.
// =================================================================================================

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// Every DECODED `apply_up` candidate for `query` against `net` (`tests/phase_c_right_to_left.rs`'s
/// own helper, reused verbatim).
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
/// (`tests/phase_c_right_to_left.rs`'s own helper, reused verbatim).
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

/// One `a -> b` rewrite rule, gated by a right environment `<OptionalSegmentSequence min="1"
/// max={max_attr}"><SimpleContext naturalClass="ncZ" /></OptionalSegmentSequence>` (one or more `z`
/// segments) -- `max_attr` is either a concrete bound (`"2"`, the bounded/must-compile fixture) or
/// `"-1"` (the DTD's own unbounded Kleene sentinel, the out-of-scope fixture). Three entries probe
/// the quantifier's own boundary behavior: `entryMin` (exactly `min` occurrences), `entryMax`
/// (exactly the bound's own `max`, when `max` is concrete), and `entryBelowMin` (zero occurrences,
/// below `min` -- the environment must NOT match, so the rule must NOT fire).
///
/// One distinct symbol value PER SEGMENT (not per natural-class membership), matching
/// `tests/phase_c_right_to_left.rs`'s own `RTL_FEATURE_ENV_XML` fixture's documented finding:
/// `pg_parse::Morpher`'s own analysis-side unapplication needs this to disambiguate segments -- a
/// grammar with NO `PhonologicalFeatureSystem` at all (every char-def's feature lanes empty) was
/// this fixture's own first-drafted shape, and it silently failed to fire the rule during
/// SYNTHESIS at all (`Morpher::generate_words` returned the root's own raw, un-rewritten spelling
/// unchanged) -- a genuine, pre-existing `pg_rules::rewrite`/zero-phonological-feature-grammar
/// interaction this fixture works around by giving every segment its own distinct feature value,
/// the same way the reference/synthetic RTL environment fixture already does, rather than a
/// Quantifier-specific finding worth its own module-doc callout.
fn quantifier_env_xml(max_attr: &str) -> String {
    format!(
        r#"<HermitCrabInput><Language><Name>QuantifierBoundedEnv</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <PhonologicalFeatureSystem>
        <SymbolicFeature id="featId"><Name>id</Name>
          <Symbols>
            <Symbol id="symA">a</Symbol><Symbol id="symB">b</Symbol><Symbol id="symZ">z</Symbol>
          </Symbols>
        </SymbolicFeature>
      </PhonologicalFeatureSystem>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations><FeatureValue feature="featId" symbolValues="symA" /></SegmentDefinition>
          <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featId" symbolValues="symB" /></SegmentDefinition>
          <SegmentDefinition id="cz"><Representations><Representation>z</Representation></Representations><FeatureValue feature="featId" symbolValues="symZ" /></SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncZ"><Name>Z</Name><Segment segment="cz" /></SegmentNaturalClass></NaturalClasses>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prQuantEnv">
          <Name>quantEnvDemo</Name>
          <PhoneticInput><PhoneticSequence><Segment segment="ca" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticOutput>
              <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence>
                <OptionalSegmentSequence min="1" max="{max_attr}"><SimpleContext naturalClass="ncZ" /></OptionalSegmentSequence>
              </PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
      <Strata>
        <Stratum characterDefinitionTable="t1" phonologicalRules="prQuantEnv">
          <Name>S</Name>
          <LexicalEntries>
            <LexicalEntry id="entryMin" partOfSpeech="posV">
              <Allomorphs><Allomorph id="alloMin"><PhoneticShape>az</PhoneticShape></Allomorph></Allomorphs>
              <Gloss>min</Gloss>
            </LexicalEntry>
            <LexicalEntry id="entryMax" partOfSpeech="posV">
              <Allomorphs><Allomorph id="alloMax"><PhoneticShape>azz</PhoneticShape></Allomorph></Allomorphs>
              <Gloss>max</Gloss>
            </LexicalEntry>
            <LexicalEntry id="entryBelowMin" partOfSpeech="posV">
              <Allomorphs><Allomorph id="alloBelowMin"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
              <Gloss>belowMin</Gloss>
            </LexicalEntry>
          </LexicalEntries>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#
    )
}

/// Compiles `rule` over `alphabet`'s table, composes after `lexc_source`, and minimizes -- the
/// shared plumbing both witnesses below use (`tests/phase_c_right_to_left.rs`'s own `compile_net`
/// helper, reused verbatim).
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
    .expect("bounded-quantifier rule must compile to Some(net)");
    assert!(skipped.is_empty(), "rule must not be skipped: {skipped:?}");
    fsm_minimize(&opts, fsm_compose(&opts, lexc_net, rule_net))
}

/// **Must-compile, oracle-exact containment.** A bounded (`min="1" max="2"`) right-environment
/// quantifier: `entryMin` ("az", exactly 1 occurrence) and `entryMax` ("azz", exactly 2
/// occurrences) both obligatorily devoice-rewrite `a -> b` (the environment holds for either
/// count); `entryBelowMin` ("a", 0 occurrences) does NOT (the environment requires at least 1), so
/// the rule must NOT fire and the root's own spelling must survive unchanged. Exercising BOTH the
/// `min` and `max` boundary occurrence counts against the SAME rule is what actually distinguishes
/// genuine bounded (1..2) behavior from an accidental always-1 (no real quantifier effect) or
/// silently-unbounded (would also accept 3+ occurrences of an environment this fixture never
/// authors) compile.
#[test]
fn quantifier_bounded_environment_compiles_and_matches_oracle() {
    let g = load(&quantifier_env_xml("2"));
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(rule.subrules.len(), 1);

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);

    let entry_min = common::gate_template::entry_id_of(&g, "entryMin");
    let entry_max = common::gate_template::entry_id_of(&g, "entryMax");
    let entry_below_min = common::gate_template::entry_id_of(&g, "entryBelowMin");

    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let entries: HashSet<LexEntryId> = [entry_min, entry_max, entry_below_min]
        .into_iter()
        .collect();
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(&g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(&g, usize::MAX);

    let allowed: HashSet<u32> = [
        g.entries[entry_min.0 as usize].morpheme.0,
        g.entries[entry_max.0 as usize].morpheme.0,
        g.entries[entry_below_min.0 as usize].morpheme.0,
    ]
    .into_iter()
    .collect();

    // --- min boundary: exactly 1 occurrence ("az" -> "bz"). ---
    let query_min = alphabet.encode_query("bz").expect("'bz' must segment");
    let fst_min = fst_candidate_set(&net, &query_min);
    let oracle_min = oracle_candidate_set(&morpher, "bz", &allowed);
    assert_eq!(
        oracle_min.len(),
        1,
        "oracle must recall entryMin for 'bz' (1 z, within [1,2]): {oracle_min:?}"
    );
    assert_eq!(
        fst_min, oracle_min,
        "CONTAINMENT for 'bz' (min-boundary, 1 occurrence)"
    );

    // --- max boundary: exactly 2 occurrences ("azz" -> "bzz"). ---
    let query_max = alphabet.encode_query("bzz").expect("'bzz' must segment");
    let fst_max = fst_candidate_set(&net, &query_max);
    let oracle_max = oracle_candidate_set(&morpher, "bzz", &allowed);
    assert_eq!(
        oracle_max.len(),
        1,
        "oracle must recall entryMax for 'bzz' (2 z's, within [1,2]): {oracle_max:?}"
    );
    assert_eq!(
        fst_max, oracle_max,
        "CONTAINMENT for 'bzz' (max-boundary, 2 occurrences)"
    );

    // Both roots' own RAW (un-rewritten) spellings must never surface (obligatory rule, and both
    // occurrence counts satisfy the environment).
    let oracle_raw_min = oracle_candidate_set(&morpher, "az", &allowed);
    assert!(
        oracle_raw_min.is_empty(),
        "'az' (obligatorily rewritten) must have no oracle analysis: {oracle_raw_min:?}"
    );
    let oracle_raw_max = oracle_candidate_set(&morpher, "azz", &allowed);
    assert!(
        oracle_raw_max.is_empty(),
        "'azz' (obligatorily rewritten) must have no oracle analysis: {oracle_raw_max:?}"
    );

    // --- below min: 0 occurrences ("a" alone) -- environment does NOT hold, rule must NOT fire. ---
    let query_below = alphabet.encode_query("a").expect("'a' must segment");
    let fst_below = fst_candidate_set(&net, &query_below);
    let oracle_below = oracle_candidate_set(&morpher, "a", &allowed);
    assert_eq!(
        oracle_below.len(),
        1,
        "oracle must recall entryBelowMin unchanged for 'a' (0 z's, below min=1): {oracle_below:?}"
    );
    assert_eq!(
        fst_below, oracle_below,
        "CONTAINMENT for 'a' (below-min: the quantifier's own min correctly gates the rule off)"
    );
}

/// **Was out-of-scope; now compiles, oracle-exact containment** (`openspec/changes/
/// build-unbounded-quantifier-support`): same shape as
/// `quantifier_bounded_environment_compiles_and_matches_oracle` above, but the right-environment
/// quantifier's own `max` is the DTD's unbounded Kleene sentinel (`max="-1"`) --
/// `pg_foma::replace::pattern_slots` now ACCEPTS this (`crate::replace::Slot::Repeat`'s
/// `max: Option<u32>` widening, foma's native `E^>N` operator), so the rule compiles instead of
/// being `skipped`. Mirrors the bounded fixture's own min/below-min boundary containment proof
/// against `pg_parse::Morpher`, and additionally proves GENUINE unboundedness directly at the FST
/// level (a 3rd occurrence count -- above the bounded fixture's own `max="2"` -- still matches,
/// which a merely-widened-but-still-secretly-bounded compile would fail).
#[test]
fn quantifier_unbounded_environment_compiles_and_matches_oracle() {
    let g = load(&quantifier_env_xml("-1"));
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(rule.subrules.len(), 1);
    assert!(
        rule.subrules[0].right_env.is_some(),
        "the demo rule's own right environment must be present"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);

    let entry_min = common::gate_template::entry_id_of(&g, "entryMin");
    let entry_max = common::gate_template::entry_id_of(&g, "entryMax");
    let entry_below_min = common::gate_template::entry_id_of(&g, "entryBelowMin");

    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let entries: HashSet<LexEntryId> = [entry_min, entry_max, entry_below_min]
        .into_iter()
        .collect();
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(&g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(&g, usize::MAX);

    let allowed: HashSet<u32> = [
        g.entries[entry_min.0 as usize].morpheme.0,
        g.entries[entry_max.0 as usize].morpheme.0,
        g.entries[entry_below_min.0 as usize].morpheme.0,
    ]
    .into_iter()
    .collect();

    // --- min boundary: exactly 1 occurrence ("az" -> "bz"). ---
    let query_min = alphabet.encode_query("bz").expect("'bz' must segment");
    let fst_min = fst_candidate_set(&net, &query_min);
    let oracle_min = oracle_candidate_set(&morpher, "bz", &allowed);
    assert_eq!(
        oracle_min.len(),
        1,
        "oracle must recall entryMin for 'bz' (1 z, satisfies min=1..unbounded): {oracle_min:?}"
    );
    assert_eq!(
        fst_min, oracle_min,
        "CONTAINMENT for 'bz' (min-boundary, 1 occurrence)"
    );

    // --- 2 occurrences ("azz" -> "bzz") -- also within the unbounded range. ---
    let query_two = alphabet.encode_query("bzz").expect("'bzz' must segment");
    let fst_two = fst_candidate_set(&net, &query_two);
    let oracle_two = oracle_candidate_set(&morpher, "bzz", &allowed);
    assert_eq!(
        oracle_two.len(),
        1,
        "oracle must recall entryMax for 'bzz' (2 z's, satisfies min=1..unbounded): {oracle_two:?}"
    );
    assert_eq!(fst_two, oracle_two, "CONTAINMENT for 'bzz' (2 occurrences)");

    // Both roots' own RAW (un-rewritten) spellings must never surface (obligatory rule).
    let oracle_raw_min = oracle_candidate_set(&morpher, "az", &allowed);
    assert!(
        oracle_raw_min.is_empty(),
        "'az' (obligatorily rewritten) must have no oracle analysis: {oracle_raw_min:?}"
    );
    let oracle_raw_two = oracle_candidate_set(&morpher, "azz", &allowed);
    assert!(
        oracle_raw_two.is_empty(),
        "'azz' (obligatorily rewritten) must have no oracle analysis: {oracle_raw_two:?}"
    );

    // --- below min: 0 occurrences ("a" alone) -- environment does NOT hold, rule must NOT fire. ---
    let query_below = alphabet.encode_query("a").expect("'a' must segment");
    let fst_below = fst_candidate_set(&net, &query_below);
    let oracle_below = oracle_candidate_set(&morpher, "a", &allowed);
    assert_eq!(
        oracle_below.len(),
        1,
        "oracle must recall entryBelowMin unchanged for 'a' (0 z's, below min=1): {oracle_below:?}"
    );
    assert_eq!(
        fst_below, oracle_below,
        "CONTAINMENT for 'a' (below-min: the quantifier's own min correctly gates the rule off)"
    );

    // Post-`build-unbounded-quantifier-support`: no longer skipped at all.
    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let composed = compile_and_compose_rules_with_budget(
        &FomaOptions::default(),
        &g,
        &alphabet,
        &[&g.prules[0]],
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("compile must not hit any budget: {e}"));

    assert!(
        skipped.is_empty(),
        "an unbounded environment quantifier must no longer be skipped: {skipped:?}"
    );
    let rule_only_net =
        composed.expect("an unbounded environment quantifier must compile to a real network");
    assert_eq!(tuple_reports.len(), 1, "one compiled (alpha-free) rule");

    // --- GENUINE unboundedness, FST-only against the BARE rule net (no lexicon involved -- the
    // full lexc-composed `net` above has no LexicalEntry spelling 3 z's at all, so testing THAT
    // would conflate lexicon coverage with environment matching). Applied "down" (underlying "a"
    // side -> surface "b" side, this file's own `Dir::LeftToRight`-style plain rule convention):
    // 3 occurrences of 'z' (above the bounded fixture's own max=2) must STILL obligatorily rewrite
    // -- an accidentally-still-capped compile would instead pass "azzz" through unchanged. ---
    let underlying_three = alphabet
        .encode_query("azzz")
        .expect("'azzz' must segment against this fixture's own table");
    let expected_surface_three = alphabet
        .encode_query("bzzz")
        .expect("'bzzz' must segment against this fixture's own table");
    let mut h = apply_init(&rule_only_net);
    assert_eq!(
        apply_down(&mut h, Some(&underlying_three)),
        Some(expected_surface_three),
        "3 occurrences (above the bounded fixture's own max=2) must still satisfy an UNBOUNDED \
         right-environment and obligatorily rewrite -- an accidentally-still-capped compile would \
         leave 'azzz' unchanged instead"
    );
}
