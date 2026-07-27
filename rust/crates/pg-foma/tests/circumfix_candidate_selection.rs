//! Containment + order-independence tests for census C1/C3
//! (`docs/conformance/circumfix-structural-composite-census.md`,
//! `openspec/changes/plan-construct-coverage-completion` tasks 4.3a/4.3b).
//!
//! ## C1 (`emit::rule_role`/`emit::is_structural_rule` — non-first-allomorph selection)
//! [`non_first_allomorph_circumfix_recall_parity`] is the proposer-to-confirm containment check for
//! `conformance-staging/edge-cases/circumfix-non-first-allomorph-selection` (the staged fixture: one
//! rule, allomorph 0 an ordinary suffix, allomorph 1 — declared SECOND — circumfix-shaped).
//! [`circumfix_allomorph_selection_is_order_independent`] is the invariant the bug violated: the
//! SAME rule, with its two allomorphs declared in the OTHER order, must be admitted into
//! [`pg_foma::emit::build_structural_composites`] (via [`pg_foma::emit::composite_candidate_rules`]'s
//! public `structural_candidate_count`) and pass the identical containment check. These two inline
//! grammars are hand-authored Rust string constants (mirroring `phase_c_circumfix.rs`'s own
//! `ORDERED_MULTI_INSERT_XML`/`NULL_ROLE_STRUCTURAL_DROP_XML` precedent for an internal-invariant
//! check that is not itself a new conformance corpus entry) rather than a second staged fixture.
//!
//! ## C3 (`emit::classify_affix` — infix-preempts-circumfix precedence)
//! [`circumfix_infix_interior_action_recall_parity`] is the proposer-to-confirm containment check
//! for `conformance-staging/edge-cases/circumfix-infix-interior-action-precedence` (the staged
//! fixture: one allomorph that is simultaneously circumfixing AND infixing).
//! [`circumfix_infix_ownership_handoff_is_clean`] checks the OTHER mechanism's own candidate set
//! (`crate::preexpand`, read via the same public [`pg_foma::emit::composite_candidate_rules`]
//! diagnostic) drops this rule cleanly the moment it reclassifies `CircumfixPrefix` — so the two
//! composite mechanisms never both claim it and never both drop it.

mod common;

use std::path::PathBuf;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;

use pg_foma::emit;
use pg_foma::tags;
use pg_grammar::model::{Grammar, MorphemeId};
use pg_parse::{Morpher, ParseOptions};

use common::gate_template::recall_reachable;

/// Repo root, from this crate's own `CARGO_MANIFEST_DIR` (`rust/crates/pg-foma`) — mirrors
/// `tests/exercises_tag_liveness.rs`'s own `repo_root()` (never a path relative to the process CWD,
/// which differs between `cargo test` and a bare test-binary invocation).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Loads a staged fixture's `grammar.xml` directly off disk — this test exercises the SAME grammar
/// `conformance_fixtures_gate.rs` replays against the oracle, never a second, drifting inline copy.
fn load_staged(name: &str) -> Grammar {
    let path = repo_root()
        .join("conformance-staging/edge-cases")
        .join(name)
        .join("grammar.xml");
    let xml = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("{name}: grammar failed to load: {e}\n{xml}"))
}

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// Re-derives the real tag sequence(s) for `surface` by re-parsing it against `morpher`'s OWN
/// grammar — mirrors `phase_c_circumfix.rs`'s own `tag_sequences_for` (the oracle's own analysis of
/// its own output, never a hand-derived guess at tag order).
fn tag_sequences_for(g: &Grammar, morpher: &Morpher, surface: &str) -> Vec<Vec<String>> {
    let popts = ParseOptions::default();
    let outcome = morpher.parse_word_opts(surface, &popts);
    let width = tags::tag_width(g.morphemes.len());
    outcome
        .structured
        .iter()
        .map(|a| {
            a.morpheme_ids
                .iter()
                .enumerate()
                .map(|(i, &m)| {
                    let mid = MorphemeId(m);
                    if i as i32 == a.root_morpheme_index {
                        tags::root_tag_text(mid, width)
                    } else {
                        tags::morph_tag_text(mid, width)
                    }
                })
                .collect()
        })
        .collect()
}

/// The standard Stage-2 containment shape: `emit::emit`'s own compiled net must be fully covered
/// (`report.uncovered` empty — a rule that only PARTIALLY reaches `build_structural_composites`
/// would still show its non-covered allomorphs here), then every analysis the REAL confirm engine
/// (`pg_parse::Morpher`) finds for `surface` must be reachable in that compiled net.
fn assert_full_containment(g: &Grammar, surface: &str) {
    let emit_result = emit::emit(g);
    assert!(
        emit_result.report.uncovered.is_empty(),
        "{surface:?}: grammar must be fully covered by the enumeration path: {:?}",
        emit_result.report.uncovered
    );
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, &emit_result.lexc_source)
        .unwrap_or_else(|| panic!("emitted lexc must compile:\n{}", emit_result.lexc_source));

    let morpher = Morpher::new(g, 20_000);
    let tag_sequences = tag_sequences_for(g, &morpher, surface);
    assert!(
        !tag_sequences.is_empty(),
        "oracle word {surface:?} must parse against its own grammar -- oracle/parser \
         inconsistency, not a recall question"
    );
    let normalized = pg_grammar::nfd::nfd(surface);
    let any_reachable = tag_sequences
        .iter()
        .any(|tags| recall_reachable(&net, &normalized, tags));
    assert!(
        any_reachable,
        "{surface:?} must be reachable with its own real tag sequence -- the census gap this \
         fixture pins is not closed"
    );
}

// =================================================================================================
// C1: circumfix-non-first-allomorph-selection
// =================================================================================================

#[test]
fn non_first_allomorph_circumfix_recall_parity() {
    let g = load_staged("circumfix-non-first-allomorph-selection");
    // Allomorph 0 (ordinary suffix): over-inclusion once the rule is admitted must stay harmless.
    assert_full_containment(&g, "mits");
    // Allomorph 1 (circumfix, declared SECOND): the load-bearing census C1 case.
    assert_full_containment(&g, "kemitan");
}

/// The SAME rule shape as the staged fixture above (allomorph 0 = ordinary suffix "-s", allomorph 1
/// = circumfix "ke-...-an"), inline so the order-independence variant below can swap the two
/// `MorphologicalSubrule` blocks without mutating the staged, oracle-verified corpus fixture.
const SUFFIX_THEN_CIRCUMFIX_XML: &str = r#"<HermitCrabInput><Language><Name>OrderSuffixThenCircumfix</Name>
  <PartsOfSpeech><PartOfSpeech id="posRoot"><Name>root</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cE"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cN"><Representations><Representation>n</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrMixed">
      <Name>Main</Name>
      <MorphologicalRuleDefinitions>
        <MorphologicalRule id="mrMixed" requiredPartsOfSpeech="posRoot" outputPartOfSpeech="posRoot">
          <Name>mixed</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="subSuffix">
              <MorphologicalInput><PhoneticSequence id="stemA"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="stemA" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule>
            <MorphologicalSubrule id="subCircum">
              <MorphologicalInput><PhoneticSequence id="stemB"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><InsertSegments><PhoneticShape>ke</PhoneticShape></InsertSegments><CopyFromInput index="stemB" /><InsertSegments><PhoneticShape>an</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
          <MorphemeId>MIXED</MorphemeId>
        </MorphologicalRule>
      </MorphologicalRuleDefinitions>
      <LexicalEntries>
        <LexicalEntry id="eRoot" partOfSpeech="posRoot">
          <Allomorphs><Allomorph id="aRoot"><PhoneticShape>mit</PhoneticShape></Allomorph></Allomorphs>
          <MorphemeId>ROOT</MorphemeId>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

/// IDENTICAL rule to [`SUFFIX_THEN_CIRCUMFIX_XML`], except the two `MorphologicalSubrule` blocks
/// are swapped: allomorph 0 is now the circumfix, allomorph 1 the ordinary suffix. Under the OLD
/// (pre-C1-fix) code this order already worked (census's own "opposite order is safe" finding,
/// since `rule_role`'s allomorph-0 view already reports `Role::CircumfixPrefix` here) — the point
/// of this test is that BOTH orders must now be selected IDENTICALLY, which is the invariant the
/// bug violated (before the fix, only this order passed).
const CIRCUMFIX_THEN_SUFFIX_XML: &str = r#"<HermitCrabInput><Language><Name>OrderCircumfixThenSuffix</Name>
  <PartsOfSpeech><PartOfSpeech id="posRoot"><Name>root</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cE"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cN"><Representations><Representation>n</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrMixed">
      <Name>Main</Name>
      <MorphologicalRuleDefinitions>
        <MorphologicalRule id="mrMixed" requiredPartsOfSpeech="posRoot" outputPartOfSpeech="posRoot">
          <Name>mixed</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="subCircum">
              <MorphologicalInput><PhoneticSequence id="stemB"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><InsertSegments><PhoneticShape>ke</PhoneticShape></InsertSegments><CopyFromInput index="stemB" /><InsertSegments><PhoneticShape>an</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule>
            <MorphologicalSubrule id="subSuffix">
              <MorphologicalInput><PhoneticSequence id="stemA"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="stemA" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
          <MorphemeId>MIXED</MorphemeId>
        </MorphologicalRule>
      </MorphologicalRuleDefinitions>
      <LexicalEntries>
        <LexicalEntry id="eRoot" partOfSpeech="posRoot">
          <Allomorphs><Allomorph id="aRoot"><PhoneticShape>mit</PhoneticShape></Allomorph></Allomorphs>
          <MorphemeId>ROOT</MorphemeId>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

#[test]
fn circumfix_allomorph_selection_is_order_independent() {
    let g_a = load(SUFFIX_THEN_CIRCUMFIX_XML);
    let g_b = load(CIRCUMFIX_THEN_SUFFIX_XML);

    let diag_a = emit::composite_candidate_rules(&g_a);
    let diag_b = emit::composite_candidate_rules(&g_b);
    assert_eq!(
        diag_a.structural_candidate_count, 1,
        "order A (suffix declared first) must admit mrMixed as a structural candidate"
    );
    assert_eq!(
        diag_b.structural_candidate_count, 1,
        "order B (circumfix declared first) must admit mrMixed as a structural candidate"
    );
    assert_eq!(
        diag_a.structural_candidate_count, diag_b.structural_candidate_count,
        "declaration order of mrMixed's two allomorphs must not change whether the rule is \
         selected as a structural candidate -- this is the invariant census C1's bug violated"
    );

    // Not just the counter: full proposer-to-confirm containment for the circumfix surface, in
    // BOTH declaration orders.
    assert_full_containment(&g_a, "kemitan");
    assert_full_containment(&g_b, "kemitan");
}

// =================================================================================================
// C3: circumfix-infix-interior-action-precedence
// =================================================================================================

#[test]
fn circumfix_infix_interior_action_recall_parity() {
    let g = load_staged("circumfix-infix-interior-action-precedence");
    assert_full_containment(&g, "kebzatan");
}

/// The ownership-handoff check task 4.3b asks for: once `mrCircInfix`'s allomorph reclassifies
/// `CircumfixPrefix` (instead of `Infix`), `crate::preexpand`'s own candidate set must drop it
/// cleanly (read via the same public [`emit::composite_candidate_rules`] diagnostic this crate
/// already exposes for exactly this kind of cross-mechanism check) -- never double-claimed by both
/// mechanisms, never silently dropped by both.
#[test]
fn circumfix_infix_ownership_handoff_is_clean() {
    let g = load_staged("circumfix-infix-interior-action-precedence");
    let diag = emit::composite_candidate_rules(&g);
    assert!(
        diag.preexpand_candidates.is_empty(),
        "mrCircInfix must NOT be claimed by crate::preexpand's own candidate set once it \
         classifies CircumfixPrefix (Infix's old claim must be relinquished): {:?}",
        diag.preexpand_candidates
    );
    assert_eq!(
        diag.structural_candidate_count, 1,
        "mrCircInfix must be exactly the one structural-composite candidate in this grammar"
    );
}
