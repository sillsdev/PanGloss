//! `openspec/changes/cover-template-truncation-reduplication`: the in-scope reduplication-peel
//! witness plus the deep/nested-chain budget-refusal witness, at the peeler-to-confirm boundary
//! (design.md's own decision: "Test reduplication at the peeler-to-confirm boundary"). Companion
//! to `crate::peel`'s own module doc ("Chain depth and nested reduplication") and its `#[cfg(test)]`
//! unit tests, which exercise the SAME mechanism from inside the crate (this file proves the public
//! API + the real `machine/conformance` fixture from outside it, the way every other `f*_gate.rs`
//! file in this crate already does).
//!
//! ## In-scope: single-layer full-stem reduplication (must compile/propose, oracle containment)
//! `machine/conformance/languages/suffixing-extension-slot-ordering`'s `mrRedup` (full-stem SUFFIX
//! reduplication, `redupMorphType="suffix"`) is an existing, oracle-verified, in-repo conformance
//! fixture whose own `words.yaml` comment documents "ReduplicationHint, zero coverage today" for
//! the FST proposer -- `kimbiakimbia`, `KIMB+RED`. [`kimbiakimbia_reduplication_is_recovered_with_oracle_containment`]
//! proves [`pg_foma::composite::FomaAnalyzer`] (propose UNION peel, confirmed against
//! `pg_parse::Morpher`) now recovers it, CONTAINMENT-checked against the very oracle that fixture's
//! own ground truth was hand-derived from (never merely "non-empty" -- every FST-confirmed analysis
//! must be one the oracle itself accepts for this word); empirically, for this specific
//! construct/word the result is stronger than mere containment -- EXACT set equality AND matching
//! multiplicity (both sides: exactly 1 analysis, identical signature).
//!
//! ## Out-of-scope / deep-chain: budget-refused deterministically, never a silent recall claim
//! A hand-built synthetic grammar (reduplication-shaped RHS only -- no lexicon/phonology needed;
//! `crate::peel::ReduplicationPeeler`'s own scan/recursion is independent of both, exactly like
//! `crate::peel`'s own unit tests) exercises a genuinely self-similar word deep enough to need more
//! nested reduplication layers than a small configured `ComposeBudget::chain_depth_cap` admits --
//! [`deep_self_similar_chain_is_refused_deterministically`] proves the refusal is a typed,
//! deterministic `ComposeError::ChainDepthExceeded`, never a hang, a panic, or a silently-truncated
//! candidate set. Synthetic/delanguaged per `openspec/changes/STAGING.md`'s "Hard rule: synthetic
//! data only" -- this fixture is authored purely in this file, named by the construct it stresses
//! (self-similar chain depth), not by any language.

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::composite::FomaAnalyzer;
use pg_foma::peel::ReduplicationPeeler;
use pg_foma::tags::Candidate;
use pg_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, Grammar, MRuleId, MorphRuleDef,
    MorphRuleOrder, MorphemeId, MprSet, OutputAction, PartRef, ReduplicationHint, StratumDef,
    TableId, VarTable,
};
use pg_parse::{Morpher, ParseOptions};

fn load(path: &str) -> Grammar {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../machine/conformance")
        .join(path);
    let xml = std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {path}: {e}"))
}

// =================================================================================================
// In-scope: single-layer reduplication, oracle containment.
// =================================================================================================

/// See module doc's "In-scope" section. Containment (never mere non-emptiness): every FST-side
/// confirmed analysis for "kimbiakimbia" must be one `pg_parse::Morpher` itself accepts for the
/// same word -- the propose-and-confirm invariant (ADR 0001) checked at the exact boundary
/// design.md names for this construct.
#[test]
fn kimbiakimbia_reduplication_is_recovered_with_oracle_containment() {
    let g = load("languages/suffixing-extension-slot-ordering/grammar.xml");

    let morpher = Morpher::new(&g, usize::MAX);
    let oracle = morpher.parse_word_opts("kimbiakimbia", &ParseOptions::default());
    assert!(
        !oracle.structured.is_empty(),
        "precondition: the oracle itself must confirm \"kimbiakimbia\" (per this fixture's own \
         words.yaml, signature \"KIMB+RED|kimbiakimbia\") for this containment check to be \
         meaningful at all"
    );

    let mut analyzer = FomaAnalyzer::new(&g).expect("this grammar already compiles (emit.rs's own \
        bare_root_phonology_makes_post_nasal_voicing_proposable exercises the same grammar)");
    let outcome = analyzer.analyze_word("kimbiakimbia");

    assert!(
        outcome.peel_chain_depth_error.is_none(),
        "an ordinary single-layer reduplication must never hit the (default-unbounded) \
         chain-depth budget: {:?}",
        outcome.peel_chain_depth_error
    );
    assert!(
        outcome.peel_used,
        "the reduplication peel must contribute at least one candidate for this word -- this is \
         precisely the construct this fixture's own words.yaml flags as \"zero coverage today\""
    );
    assert!(
        !outcome.structured.is_empty(),
        "\"kimbiakimbia\" must confirm at least one analysis now that the peel recovers it"
    );

    let fst_sigs: std::collections::HashSet<Vec<u32>> = outcome
        .structured
        .iter()
        .map(|wa| wa.morpheme_ids.clone())
        .collect();
    let oracle_sigs: std::collections::HashSet<Vec<u32>> = oracle
        .structured
        .iter()
        .map(|wa| wa.morpheme_ids.clone())
        .collect();
    assert!(
        fst_sigs.is_subset(&oracle_sigs),
        "FST-confirmed analysis set must be CONTAINED in the oracle's own set (never an \
         over-claim); FST-only extra analyses: {:?}",
        fst_sigs.difference(&oracle_sigs).collect::<Vec<_>>()
    );
    // Empirically, for THIS construct/word the containment is actually EXACT (set equality, same
    // multiplicity) -- a strictly stronger property than the subset check above, checked
    // separately so a future divergence (e.g. the oracle someday finding a second analysis this
    // peel doesn't) fails this specific assertion, not the general containment one.
    assert_eq!(
        fst_sigs, oracle_sigs,
        "for kimbiakimbia specifically, the FST-confirmed set is not just contained in but \
         EQUAL to the oracle's own set"
    );
    assert_eq!(
        outcome.structured.len(),
        oracle.structured.len(),
        "multiplicity (analysis COUNT, not just distinct-signature set) must match the oracle too"
    );
}

// =================================================================================================
// Out-of-scope / deep-chain: deterministic budget refusal.
// =================================================================================================

/// A minimal, hand-built grammar carrying nothing but one `AffixProcessRule` whose RHS classifies
/// `Role::Reduplication` (`OutputAction::Copy(PartRef::Input(0))` twice, `crate::emit::
/// classify_affix`'s own trigger) -- everything `ReduplicationPeeler::new` reads. Mirrors `crate::
/// peel`'s own `#[cfg(test)] mod tests::minimal_redup_grammar` (duplicated rather than shared: an
/// integration test file is a separate compiled crate from `pg-foma`'s own `src`, so a
/// `#[cfg(test)]`-only helper there is not visible here -- the same reason `redup_and_free_
/// fluctuation_gate.rs`'s own doc gives for its small inlined builders).
fn minimal_redup_grammar() -> Grammar {
    const MINIMAL_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PeelChainDepthBoundaryFixture</Name>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
  </Language>
</HermitCrabInput>"#;
    let mut g = pg_grammar::load(MINIMAL_XML).expect("minimal fixture loads");
    let redup_mrule = MRuleId(g.mrules.len() as u32);
    g.mrules.push(MorphRuleDef::AffixProcess(AffixProcessRuleDef {
        morpheme: MorphemeId(0),
        name: Some("redupChainDepthBoundaryFixture".to_string()),
        blockable: false,
        partial: false,
        max_apps: 1,
        required_syn_fs: pg_featstruct::FsId(0),
        out_syn_fs: pg_featstruct::FsId(0),
        obligatory_features: vec![],
        required_stem_name: None,
        is_template_rule: false,
        allomorphs: vec![AffixAllomorphDef {
            id: AllomorphId(0),
            environments: vec![],
            co_occurrence: vec![],
            required_syn_fs: pg_featstruct::FsId(0),
            vars: VarTable::default(),
            required_mpr: MprSet::EMPTY,
            excluded_mpr: MprSet::EMPTY,
            out_mpr: MprSet::EMPTY,
            redup_hint: ReduplicationHint::Suffix,
            lhs: vec![],
            rhs: vec![
                OutputAction::Copy(PartRef::Input(0)),
                OutputAction::Copy(PartRef::Input(0)),
            ],
            properties: vec![],
        }],
    }));
    g.strata.push(StratumDef {
        name: Some("chainDepthBoundaryStratum".to_string()),
        table: TableId(0),
        mrule_order: MorphRuleOrder::Linear,
        prules: vec![],
        mrules: vec![redup_mrule],
        templates: vec![],
        entries: vec![],
    });
    g
}

/// See module doc's "Out-of-scope / deep-chain" section: a genuinely self-similar (every character
/// identical) word matches this module's scans at many positions simultaneously, and nested
/// recursion (`crate::peel`'s own module doc) is exercised layer after layer. Under a small cap,
/// this MUST refuse deterministically rather than hang or silently truncate.
#[test]
fn deep_self_similar_chain_is_refused_deterministically() {
    let g = minimal_redup_grammar();
    let peeler = ReduplicationPeeler::new(&g);
    assert!(peeler.has_redup_rules());
    let mut propose = |_: &str| -> Vec<Candidate> { Vec::new() };
    let budget = ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None)
        .with_chain_depth_cap(3);
    let word = "a".repeat(16);
    let err = peeler
        .peel_candidates(&g, &word, &budget, &mut propose)
        .expect_err(
            "a 16-char monochar word's self-similar structure genuinely needs more than 3 nested \
             reduplication layers; a cap of 3 must refuse it deterministically",
        );
    let msg = err.to_string();
    assert!(
        msg.contains("chain-depth"),
        "the error message must be the typed chain-depth diagnostic, got: {msg}"
    );
}

/// The same construct, with a cap generous enough to admit it, must NOT refuse -- proving the
/// budget only trips on a genuinely deep chain, never on the construct's mere existence.
#[test]
fn deep_self_similar_chain_succeeds_under_a_generous_cap() {
    let g = minimal_redup_grammar();
    let peeler = ReduplicationPeeler::new(&g);
    let mut propose = |_: &str| -> Vec<Candidate> { Vec::new() };
    let budget = ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None)
        .with_chain_depth_cap(64);
    let word = "a".repeat(10);
    peeler
        .peel_candidates(&g, &word, &budget, &mut propose)
        .expect("a generous cap must admit this word in full");
}
