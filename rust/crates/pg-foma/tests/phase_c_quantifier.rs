//! GATE: quantifier / `OptionalSegmentSequence` compile gate: pins the loader/compiler's ACTUAL disposition per shape (bounded now compiles via `Slot::Repeat`'s `^{min,max}`; unbounded now compiles via its `*`/`^>N` widening), placing the containment fixture in an environment rather than the LHS/RHS focus since only the former is free of a pre-existing confirm-engine width-mismatch gap.
//! See `docs/research/pg-foma-phase-c-quantifier-gate-notes.md` for the width-mismatch gap and why the environment placement is load-bearing, not arbitrary.

mod common;

use std::collections::HashSet;

use foma::apply::{apply_down, apply_init};
use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;

use pg_foma::replace::{compile_and_compose_rules, SegAlphabet};
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying_filtered;
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

/// Was honestly skipped; now compiles. The generator always mints a genuinely UNBOUNDED (`max="-1"`) quantifier occupying the rule's WHOLE LHS focus; `Slot::Repeat`'s widening now renders it, so this compiles to `Some(net)` -- this unit test exercises the FST COMPILE side only, not confirm-side containment (a separate, pre-existing gap, see `crate::replace`'s "Confirm-engine finding").
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

    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let composed =
        compile_and_compose_rules(&opts, &g, &alphabet, &ro, &mut skipped, &mut tuple_reports);

    // The quantifier-bearing rule is no longer skipped at all -- it compiles to a real network, alpha-free (a trivial 1-entry tuple report).
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

// Bounded quantifier, IN AN ENVIRONMENT (see this file's top doc): synthetic fixtures, named by construct.

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// Every DECODED `apply_up` candidate for `query` against `net` (`tests/phase_c_right_to_left.rs`'s helper, reused verbatim).
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

/// The full-HC oracle's own candidate set for `surface`, restricted to `allowed_morphemes` (`tests/phase_c_right_to_left.rs`'s helper, reused verbatim).
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

/// One `a -> b` rewrite rule gated by a right environment of one-or-more `z` segments, `max_attr` either a concrete bound or the DTD's unbounded Kleene sentinel `"-1"`; three entries (`entryMin`/`entryMax`/`entryBelowMin`) probe the quantifier's boundary behavior.
/// See `docs/research/pg-foma-phase-c-quantifier-gate-notes.md` for why every segment gets its own distinct feature value.
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

/// Compiles `rule` over `alphabet`'s table, composes after `lexc_source`, and minimizes -- shared plumbing both witnesses below use (`tests/phase_c_right_to_left.rs`'s `compile_net` helper, reused verbatim).
fn compile_net(
    g: &Grammar,
    alphabet: &SegAlphabet,
    rule: &PhonRuleDef,
    lexc_source: &str,
) -> foma::types::Fsm {
    let opts = FomaOptions::default();
    let lexc_net = fsm_lexc_parse_string(&opts, None, lexc_source)
        .unwrap_or_else(|| panic!("lexc must compile:\n{lexc_source}"));
    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let rule_net = compile_and_compose_rules(
        &opts,
        g,
        alphabet,
        &[rule],
        &mut skipped,
        &mut tuple_reports,
    )
    .expect("bounded-quantifier rule must compile to Some(net)");
    assert!(skipped.is_empty(), "rule must not be skipped: {skipped:?}");
    fsm_minimize(&opts, fsm_compose(&opts, lexc_net, rule_net))
}

/// Must-compile, oracle-exact containment for a bounded (`min="1" max="2"`) right-environment quantifier: exercising BOTH the `min` and `max` boundary counts against the SAME rule is what distinguishes genuine bounded behavior from an accidental always-1 or silently-unbounded compile.
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

    let entries: HashSet<LexEntryId> = [entry_min, entry_max, entry_below_min]
        .into_iter()
        .collect();
    let uemit = emit_underlying_filtered(&g, &alphabet, Some(&entries))
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

    // Both roots' own RAW (un-rewritten) spellings must never surface (obligatory rule, both occurrence counts satisfy the environment).
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

/// Was out-of-scope; now compiles, oracle-exact containment: same shape as the bounded fixture, but `max` is the DTD's unbounded sentinel `"-1"`, and this additionally proves GENUINE unboundedness (a 3rd occurrence, above the bounded fixture's `max="2"`, still matches).
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

    let entries: HashSet<LexEntryId> = [entry_min, entry_max, entry_below_min]
        .into_iter()
        .collect();
    let uemit = emit_underlying_filtered(&g, &alphabet, Some(&entries))
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
    let composed = compile_and_compose_rules(
        &FomaOptions::default(),
        &g,
        &alphabet,
        &[&g.prules[0]],
        &mut skipped,
        &mut tuple_reports,
    );

    assert!(
        skipped.is_empty(),
        "an unbounded environment quantifier must no longer be skipped: {skipped:?}"
    );
    let rule_only_net =
        composed.expect("an unbounded environment quantifier must compile to a real network");
    assert_eq!(tuple_reports.len(), 1, "one compiled (alpha-free) rule");

    // GENUINE unboundedness, FST-only against the BARE rule net (no lexicon involved, since testing through the lexicon would conflate lexicon coverage with environment matching): 3 occurrences of 'z' must still obligatorily rewrite.
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
