//! Reduplication at the peeler-to-confirm boundary: in-scope single-layer oracle-containment recovery, plus a deep/nested-chain deterministic budget refusal (synthetic fixture, named by construct).

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::peel::ReduplicationPeeler;
use pg_foma::tags::Candidate;
use pg_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, Grammar, MRuleId, MorphRuleDef,
    MorphRuleOrder, MorphemeId, MprSet, OutputAction, PartRef, Pattern, ReduplicationHint,
    StratumDef, TableId, VarTable,
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

/// Requires the peeler to propose every oracle analysis for "kimbiakimbia".
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

    // Isolate peeling because this fixture's unrelated unbounded rule correctly refuses construction.
    let peeler = ReduplicationPeeler::new(&g);
    assert!(peeler.has_redup_rules());
    let mut propose = |residual: &str| {
        let parsed = morpher.parse_word_opts(residual, &ParseOptions::default());
        parsed
            .structured
            .into_iter()
            .map(|analysis| Candidate {
                morphemes: analysis.morpheme_ids.into_iter().map(MorphemeId).collect(),
                root_index: analysis.root_morpheme_index,
            })
            .collect::<Vec<_>>()
    };
    let recovered = peeler
        .peel_candidates(&g, "kimbiakimbia", &ComposeBudget::from_env(), &mut propose)
        .expect("ordinary single-layer reduplication must not hit the chain-depth budget");
    assert!(
        !recovered.is_empty(),
        "the reduplication peel must recover this word"
    );

    let proposal_sigs: std::collections::HashSet<Vec<u32>> = recovered
        .iter()
        .map(|candidate| candidate.morphemes.iter().map(|id| id.0).collect())
        .collect();
    let oracle_sigs: std::collections::HashSet<Vec<u32>> = oracle
        .structured
        .iter()
        .map(|wa| wa.morpheme_ids.clone())
        .collect();
    assert!(
        oracle_sigs.is_subset(&proposal_sigs),
        "the reduplication peeler must propose every oracle analysis; missing signatures: {:?}",
        oracle_sigs.difference(&proposal_sigs).collect::<Vec<_>>()
    );
}

// Out-of-scope / deep-chain: deterministic budget refusal.

/// A minimal, hand-built grammar carrying nothing but one `AffixProcessRule` whose RHS classifies `Role::Reduplication` -- everything `ReduplicationPeeler::new` reads.
fn minimal_redup_grammar() -> Grammar {
    const MINIMAL_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PeelChainDepthBoundaryFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
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
                // One empty input pattern plus two copies is the minimal peelable self-similar shape.
                lhs: vec![Pattern::default()],
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
