//! Containment + order-independence tests for census C1/C2/C3
//! (`docs/conformance/circumfix-structural-composite-census.md`,
//! `openspec/changes/plan-construct-coverage-completion` tasks 4.3a/4.3b/4.3c).
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
//!
//! ## C2 (`emit::classify_affix` — reduplication-preempts-circumfix precedence; task 4.3c)
//! [`circumfix_reduplication_recall_parity`] is the proposer-to-confirm containment check for
//! `conformance-staging/edge-cases/circumfix-reduplication-precedence` (the staged fixture: one
//! allomorph that is simultaneously circumfixing AND reduplicating — the same LHS part `Copy`d
//! twice, wrapped by a leading and a trailing insert). Unlike C3, this is not merely an ownership
//! relabeling of an already-correct outcome: [`peel_relinquishes_circumfix_reduplication_cleanly`]
//! checks the OTHER mechanism this Role feeds (`crate::peel::ReduplicationPeeler`, whose four scan
//! kinds are each one-sided surface matches that cannot recall a genuine wrap-both-sides-plus-
//! reduplication surface) drops the rule cleanly — `has_redup_rules()` must be `false` for this
//! grammar once `classify_affix` stops calling this shape `Role::Reduplication`.
//! [`c1_and_c3_selection_is_unperturbed_by_the_c2_fix`] re-runs C1's and C3's own staged fixtures
//! through the same public diagnostics C1/C3's own tests use, pinning that neither's selection
//! outcome moved as a side effect of the C2 fix.

mod common;

use std::path::PathBuf;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;

use pg_foma::emit;
use pg_foma::peel::ReduplicationPeeler;
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

// =================================================================================================
// C2: circumfix-reduplication-precedence (task 4.3c, jointly decided with row 11's carve-out)
// =================================================================================================

#[test]
fn circumfix_reduplication_recall_parity() {
    let g = load_staged("circumfix-reduplication-precedence");
    assert_full_containment(&g, "ketamtaman");
}

/// The ownership-handoff check task 4.3c asks for -- the OTHER direction from C1/C3's
/// `crate::preexpand` handoff: once `mrCircRedup`'s allomorph reclassifies `CircumfixPrefix`
/// (instead of `Reduplication`), `crate::peel::ReduplicationPeeler` must relinquish it cleanly, not
/// merely stop being the PREFERRED mechanism while still nominally claiming the rule.
/// `ReduplicationPeeler::new` builds its `redup_rules` list by calling `is_reduplication_rule`,
/// which itself calls `classify_affix` per allomorph (`peel.rs`) -- since this grammar's ONLY
/// `MorphologicalRule` is `mrCircRedup`, and its one allomorph no longer classifies
/// `Role::Reduplication` after the C2 fix, `has_redup_rules()` must be `false` for the whole
/// grammar. This is the stronger claim census C2's own reasoning rests on (not merely "a different
/// mechanism ALSO covers it" the way C3's `preexpand` finding turned out): the peel's four scan
/// kinds cannot recall this wrap-both-sides-plus-reduplication shape at all (see this file's own
/// top-doc and the fixture's STAGING.md), so it matters that the peel is not even attempted here.
#[test]
fn peel_relinquishes_circumfix_reduplication_cleanly() {
    let g = load_staged("circumfix-reduplication-precedence");
    let peeler = ReduplicationPeeler::new(&g);
    assert!(
        !peeler.has_redup_rules(),
        "mrCircRedup must NOT be classified as a reduplication rule by \
         crate::peel::ReduplicationPeeler once its allomorph reclassifies CircumfixPrefix -- the \
         peel's one-sided scan kinds cannot recall a genuine wrap-both-sides-plus-reduplication \
         surface, so it must relinquish this rule entirely, not merely stop being preferred"
    );

    let diag = emit::composite_candidate_rules(&g);
    assert_eq!(
        diag.structural_candidate_count, 1,
        "mrCircRedup must be exactly the one structural-composite candidate in this grammar"
    );
}

/// Task 4.3c's own required pin: C1's and C3's selection outcomes must be UNPERTURBED by the C2
/// fix. Re-runs both fixtures through the exact same public diagnostics
/// (`emit::composite_candidate_rules`) their own dedicated tests above already use, asserting the
/// identical expected values those tests assert -- if the C2 reordering had shifted anything about
/// how `classify_affix` treats a non-reduplicating RHS, either assertion below would fail.
#[test]
fn c1_and_c3_selection_is_unperturbed_by_the_c2_fix() {
    // C1: circumfix-non-first-allomorph-selection -- allomorph 1 (circumfix, declared second)
    // must still be selected, and the ordinary allomorph 0 (plain suffix) must still be harmless.
    let g_c1 = load_staged("circumfix-non-first-allomorph-selection");
    let diag_c1 = emit::composite_candidate_rules(&g_c1);
    assert_eq!(
        diag_c1.structural_candidate_count, 1,
        "C2's fix must not change C1's own structural-candidate selection count"
    );
    assert_full_containment(&g_c1, "mits");
    assert_full_containment(&g_c1, "kemitan");

    // C3: circumfix-infix-interior-action-precedence -- still CircumfixPrefix, still handed off
    // cleanly away from crate::preexpand.
    let g_c3 = load_staged("circumfix-infix-interior-action-precedence");
    let diag_c3 = emit::composite_candidate_rules(&g_c3);
    assert!(
        diag_c3.preexpand_candidates.is_empty(),
        "C2's fix must not resurrect mrCircInfix in crate::preexpand's own candidate set: {:?}",
        diag_c3.preexpand_candidates
    );
    assert_eq!(
        diag_c3.structural_candidate_count, 1,
        "C2's fix must not change C3's own structural-candidate selection count"
    );
    assert_full_containment(&g_c3, "kebzatan");
}
