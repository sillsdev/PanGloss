//! Reduplication at the peeler-to-confirm boundary: in-scope single-layer oracle-containment recovery, plus a deep/nested-chain deterministic budget refusal (synthetic fixture, named by construct).

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

// In-scope: single-layer reduplication, oracle containment.

/// Containment, never mere non-emptiness: every FST-side confirmed analysis for "kimbiakimbia" must be one `pg_parse::Morpher` itself accepts for the same word.
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

    let mut analyzer = FomaAnalyzer::new(&g).expect(
        "this grammar already compiles (emit.rs's own \
        bare_root_phonology_makes_post_nasal_voicing_proposable exercises the same grammar)",
    );
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
    // For kimbiakimbia specifically, containment is exact (set equality, same multiplicity), checked separately from the general subset check above.
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

// Out-of-scope / deep-chain: deterministic budget refusal.

/// A minimal, hand-built grammar carrying nothing but one `AffixProcessRule` whose RHS classifies `Role::Reduplication` -- everything `ReduplicationPeeler::new` reads.
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
    g.mrules
        .push(MorphRuleDef::AffixProcess(AffixProcessRuleDef {
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

/// A genuinely self-similar word matches this module's scans at many positions simultaneously, exercising nested recursion layer after layer; under a small cap it must refuse deterministically.
#[test]
fn deep_self_similar_chain_is_refused_deterministically() {
    let g = minimal_redup_grammar();
    let peeler = ReduplicationPeeler::new(&g);
    assert!(peeler.has_redup_rules());
    let mut propose = |_: &str| -> Vec<Candidate> { Vec::new() };
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    )
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

/// The same construct, with a generous cap, must not refuse -- the budget trips on chain depth, never on the construct's mere existence.
#[test]
fn deep_self_similar_chain_succeeds_under_a_generous_cap() {
    let g = minimal_redup_grammar();
    let peeler = ReduplicationPeeler::new(&g);
    let mut propose = |_: &str| -> Vec<Candidate> { Vec::new() };
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    )
    .with_chain_depth_cap(64);
    let word = "a".repeat(10);
    peeler
        .peel_candidates(&g, &word, &budget, &mut propose)
        .expect("a generous cap must admit this word in full");
}
