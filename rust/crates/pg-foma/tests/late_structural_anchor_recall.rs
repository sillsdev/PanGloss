//! Mbugwe-derived containment regression for a structural anchor reached after four ordinary rules.

mod common;

use std::path::PathBuf;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use pg_foma::analyzer::FomaProposer;
use pg_foma::composite::FomaAnalyzer;
use pg_foma::emit;
use pg_foma::peel::ReduplicationPeeler;
use pg_foma::tags;
use pg_grammar::model::{Grammar, MorphemeId};
use pg_parse::{Morpher, ParseOptions};

use common::gate_template::{mrule_id_of, recall_reachable};

fn fixture_xml() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pangloss/fst-completeness/late-structural-anchor-five-rule-chain/grammar.xml");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn load_fixture() -> Grammar {
    pg_grammar::load(&fixture_xml())
        .unwrap_or_else(|error| panic!("fixture failed to load: {error}"))
}

fn assert_surface_recalled(grammar: &Grammar, surface: &str) {
    let emitted = emit::emit(grammar);
    assert!(
        !matches!(emitted.report.tier, emit::FomaTier::Unsupported { .. }),
        "authored finite chain must close: {:?}",
        emitted.report.tier
    );
    let net = fsm_lexc_parse_string(&FomaOptions::default(), None, &emitted.lexc_source)
        .expect("finite-closure lexc must compile");

    let morpher = Morpher::new(grammar, 20_000);
    let oracle = morpher.parse_word_opts(surface, &ParseOptions::default());
    assert!(
        !oracle.structured.is_empty(),
        "full-engine oracle must analyze {surface}"
    );
    let width = tags::tag_width(grammar.morphemes.len());
    let recalled = oracle.structured.iter().any(|analysis| {
        let expected = analysis
            .morpheme_ids
            .iter()
            .enumerate()
            .map(|(index, &morpheme)| {
                let morpheme = MorphemeId(morpheme);
                if index as i32 == analysis.root_morpheme_index {
                    tags::root_tag_text(morpheme, width)
                } else {
                    tags::morph_tag_text(morpheme, width)
                }
            })
            .collect::<Vec<_>>();
        recall_reachable(&net, surface, &expected)
    });
    if !recalled {
        eprintln!(
            "surface={surface:?} tier={:?} uncovered={:?} oracle={:?}\n--- lexc ---\n{}",
            emitted.report.tier, emitted.report.uncovered, oracle.structured, emitted.lexc_source
        );
    }
    assert!(
        recalled,
        "the FST must recall a full-engine analysis of {surface}"
    );
}

fn assert_product_analysis_matches_oracle(grammar: &Grammar, surface: &str) {
    let oracle = Morpher::new(grammar, 20_000).parse_word_opts(surface, &ParseOptions::default());
    assert!(
        !oracle.structured.is_empty(),
        "oracle must analyze {surface}"
    );
    // Synthetic variants check propose/peel/confirm behavior, not artifact completeness.
    let (proposer, _) = FomaProposer::new_unproven_with_profile(grammar);
    let proposer = proposer.expect("development-only proposer must compile");
    let mut analyzer = FomaAnalyzer::from_precompiled_proposer(grammar, proposer);
    let outcome = analyzer.analyze_word(surface);
    assert_eq!(
        outcome.structured, oracle.structured,
        "propose/peel/confirm must reproduce the oracle analysis for {surface}"
    );
}

fn assert_mixed_rule_uses_structural_route(grammar: &Grammar) {
    let mixed = mrule_id_of(grammar, "mrMixed");
    let diagnostics = emit::composite_candidate_rules(grammar);
    assert!(
        diagnostics.structural_candidates.contains(&mixed.0),
        "edge-inserted partial reduplication must use structural synthesis: {:?}",
        diagnostics.structural_candidates
    );
    assert!(
        diagnostics
            .preexpand_candidates
            .iter()
            .all(|(candidate, _)| *candidate != mixed.0),
        "ordinary composite expansion must relinquish a structurally owned mixed rule: {:?}",
        diagnostics.preexpand_candidates
    );
    assert!(
        !ReduplicationPeeler::new(grammar).has_redup_rules(),
        "the generic edge scanner must relinquish a rule whose reduplication has authored edge material"
    );
}

fn assert_mixed_rule_uses_peeler(grammar: &Grammar) {
    let mixed = mrule_id_of(grammar, "mrMixed");
    let diagnostics = emit::composite_candidate_rules(grammar);
    assert!(
        !diagnostics.structural_candidates.contains(&mixed.0),
        "peel-supported reduplication must not widen structural closure: {:?}",
        diagnostics.structural_candidates
    );
    assert!(
        ReduplicationPeeler::new(grammar).has_redup_rules(),
        "the generic peeler must retain a supported reduplication shape"
    );
}

fn complex_allomorph_fixture_xml() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pangloss/fst-completeness/complex-inserted-redup-later-allomorph/grammar.xml");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn five_rule_chain_with_late_structural_anchor_reaches_finite_closure() {
    let grammar = load_fixture();
    let diagnostics = emit::composite_candidate_rules(&grammar);
    let structural = mrule_id_of(&grammar, "mr5");
    assert!(
        diagnostics.structural_candidates.contains(&structural.0),
        "the circumfix anchor must remain structurally owned: {:?}",
        diagnostics.structural_candidates
    );
    for ordinary_id in ["mr1", "mr2", "mr3", "mr4"] {
        let ordinary = mrule_id_of(&grammar, ordinary_id);
        assert!(
            !diagnostics.structural_candidates.contains(&ordinary.0),
            "an edge-decomposable anchor must not pull ordinary prefix {ordinary_id} into \
             factorial structural closure: {:?}",
            diagnostics.structural_candidates
        );
    }
    assert_surface_recalled(&grammar, "fedcbag");
}

#[test]
fn authored_multiple_application_two_is_exhausted() {
    let xml = fixture_xml().replacen(
        "<MorphologicalRule id=\"mr5\"",
        "<MorphologicalRule id=\"mr5\" multipleApplication=\"2\"",
        1,
    );
    let grammar = pg_grammar::load(&xml).expect("multipleApplication fixture must load");
    assert_surface_recalled(&grammar, "ffedcbagg");
}

#[test]
fn later_complex_allomorph_is_recalled_without_hiding_the_first() {
    let xml = complex_allomorph_fixture_xml();
    let grammar = pg_grammar::load(&xml)
        .unwrap_or_else(|error| panic!("complex-allomorph fixture failed to load: {error}"));

    assert_mixed_rule_uses_structural_route(&grammar);
    assert_product_analysis_matches_oracle(&grammar, "cheefu");
    assert_surface_recalled(&grammar, "xp");

    let emitted = emit::emit(&grammar);
    assert!(
        emitted.report.uncovered.is_empty(),
        "structural ownership must clear stale uncovered records: {:?}",
        emitted.report.uncovered
    );
    let mut analyzer = FomaAnalyzer::new(&grammar).expect("PanGloss analyzer must compile");
    let complex = analyzer.analyze_word("cheefu");
    assert!(
        !complex.peel_used,
        "the structurally owned later allomorph must not be recovered by the generic peeler"
    );
    let bare = analyzer.analyze_word("p");
    assert!(
        !bare.structured.is_empty(),
        "bare control root must analyze"
    );
    assert!(
        bare.structured
            .iter()
            .all(|analysis| analysis.morpheme_ids.len() == 1),
        "bare p must not acquire the mixed rule: {:?}",
        bare.structured
    );
}

#[test]
fn trailing_insert_around_partial_reduplication_is_recalled() {
    let xml = complex_allomorph_fixture_xml().replace(
        "<InsertSegments><PhoneticShape>ch</PhoneticShape></InsertSegments><CopyFromInput index=\"qA\" /><CopyFromInput index=\"qA\" /><CopyFromInput index=\"qB\" />",
        "<CopyFromInput index=\"qA\" /><CopyFromInput index=\"qA\" /><CopyFromInput index=\"qB\" /><InsertSegments><PhoneticShape>ch</PhoneticShape></InsertSegments>",
    );
    let grammar = pg_grammar::load(&xml)
        .unwrap_or_else(|error| panic!("trailing-insert fixture failed to load: {error}"));

    assert_mixed_rule_uses_structural_route(&grammar);
    assert_product_analysis_matches_oracle(&grammar, "eefuch");
}

#[test]
fn partial_internal_separator_reduplication_routes_structurally() {
    let xml = complex_allomorph_fixture_xml().replace(
        "<InsertSegments><PhoneticShape>ch</PhoneticShape></InsertSegments><CopyFromInput index=\"qA\" /><CopyFromInput index=\"qA\" /><CopyFromInput index=\"qB\" />",
        "<CopyFromInput index=\"qA\" /><InsertSegments><PhoneticShape>h</PhoneticShape></InsertSegments><CopyFromInput index=\"qA\" /><CopyFromInput index=\"qB\" />",
    );
    let grammar = pg_grammar::load(&xml)
        .unwrap_or_else(|error| panic!("internal-separator fixture failed to load: {error}"));

    assert_mixed_rule_uses_structural_route(&grammar);
    assert_product_analysis_matches_oracle(&grammar, "ehefu");
}

#[test]
fn base_separator_suffix_copy_stays_on_the_peeler() {
    let xml = complex_allomorph_fixture_xml()
        .replace(
            "</SegmentDefinitions></CharacterDefinitionTable>",
            "</SegmentDefinitions><BoundaryDefinitions><BoundaryDefinition id=\"bPlus\"><Representations><Representation>+</Representation></Representations></BoundaryDefinition></BoundaryDefinitions></CharacterDefinitionTable>",
        )
        .replace(
            "<InsertSegments><PhoneticShape>ch</PhoneticShape></InsertSegments><CopyFromInput index=\"qA\" /><CopyFromInput index=\"qA\" /><CopyFromInput index=\"qB\" />",
            "<CopyFromInput index=\"qA\" /><CopyFromInput index=\"qB\" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><InsertSegments><PhoneticShape>h</PhoneticShape></InsertSegments><CopyFromInput index=\"qB\" />",
        );
    let grammar = pg_grammar::load(&xml)
        .unwrap_or_else(|error| panic!("one-separator fixture failed to load: {error}"));

    assert_mixed_rule_uses_peeler(&grammar);
    assert_product_analysis_matches_oracle(&grammar, "efuhfu");
}

#[test]
fn boundary_only_edge_insertion_does_not_force_structural_routing() {
    let xml = complex_allomorph_fixture_xml()
        .replace(
            "</SegmentDefinitions></CharacterDefinitionTable>",
            "</SegmentDefinitions><BoundaryDefinitions><BoundaryDefinition id=\"bPlus\"><Representations><Representation>+</Representation></Representations></BoundaryDefinition></BoundaryDefinitions></CharacterDefinitionTable>",
        )
        .replace(
            "<InsertSegments><PhoneticShape>ch</PhoneticShape></InsertSegments><CopyFromInput index=\"qA\" /><CopyFromInput index=\"qA\" /><CopyFromInput index=\"qB\" />",
            "<InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index=\"qA\" /><CopyFromInput index=\"qA\" /><CopyFromInput index=\"qB\" />",
        );
    let grammar = pg_grammar::load(&xml)
        .unwrap_or_else(|error| panic!("boundary-only fixture failed to load: {error}"));

    assert_mixed_rule_uses_peeler(&grammar);
    assert_product_analysis_matches_oracle(&grammar, "eefu");
}

#[test]
fn bounded_role_none_predecessor_is_in_structural_closure_candidates() {
    let xml = fixture_xml()
        .replace(
            "morphologicalRules=\"mr1 mr2 mr3 mr4 mr5\"",
            "morphologicalRules=\"mrNone mr1 mr2 mr3 mr4 mr5\"",
        )
        .replace(
            "    </MorphologicalRuleDefinitions>",
            "    <MorphologicalRule id=\"mrNone\" requiredPartsOfSpeech=\"posV\" outputPartOfSpeech=\"posV\"><Name>none</Name><MorphologicalSubrules><MorphologicalSubrule id=\"sNone\"><MorphologicalInput><PhoneticSequence id=\"stemNone\"><OptionalSegmentSequence min=\"1\" max=\"-1\"><SimpleContext naturalClass=\"ncAny\" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index=\"stemNone\" /></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>NONE</MorphemeId></MorphologicalRule>\n    </MorphologicalRuleDefinitions>",
        );
    let grammar = pg_grammar::load(&xml).expect("Role::None predecessor fixture must load");
    let none = mrule_id_of(&grammar, "mrNone");
    let diagnostics = emit::composite_candidate_rules(&grammar);
    assert!(
        diagnostics.structural_candidates.contains(&none.0),
        "a bounded Role::None rule can feed a later structural anchor and must be explored: {:?}",
        diagnostics.structural_candidates
    );
}

#[test]
fn clean_suffix_after_structural_anchor_does_not_mint_another_structural_record() {
    let two_strata = fixture_xml().replace(
        "</Stratum></Strata>",
        "</Stratum><Stratum characterDefinitionTable=\"t1\"><Name>Later</Name></Stratum></Strata>",
    );
    let baseline = pg_grammar::load(&two_strata).expect("two-stratum baseline fixture must load");
    let xml = two_strata
        .replace(
            "<Stratum characterDefinitionTable=\"t1\"><Name>Later</Name></Stratum>",
            "<Stratum characterDefinitionTable=\"t1\" morphologicalRules=\"mrSuffix\"><Name>Later</Name><MorphologicalRuleDefinitions><MorphologicalRule id=\"mrSuffix\" requiredPartsOfSpeech=\"posV\" outputPartOfSpeech=\"posV\"><Name>suffix</Name><MorphologicalSubrules><MorphologicalSubrule id=\"sSuffix\"><MorphologicalInput><PhoneticSequence id=\"stemSuffix\"><OptionalSegmentSequence min=\"1\" max=\"-1\"><SimpleContext naturalClass=\"ncAny\" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index=\"stemSuffix\" /><InsertSegments><PhoneticShape>g</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>SUFFIX</MorphemeId></MorphologicalRule></MorphologicalRuleDefinitions></Stratum>",
        );
    let extended = pg_grammar::load(&xml).expect("suffix fixture must load");
    let baseline_count = emit::emit(&baseline)
        .report
        .counts
        .composite_structural_entries;
    let extended_result = emit::emit(&extended);
    assert_eq!(
        extended_result.report.counts.composite_structural_entries, baseline_count,
        "a clean ordinary suffix after the structural anchor is already reachable through its \
         ordinary emission path"
    );
    assert_surface_recalled(&extended, "fedcbagg");
}

#[test]
fn structural_slot_ordinary_slot_later_structural_slot_is_not_cut_off() {
    // The mandatory ordinary middle slot makes a one-step dirty-tail check unsound.
    let xml = fixture_xml()
        .replace(
            "<MorphologicalRule id=\"mr5\"",
            "<MorphologicalRule id=\"mr5\" multipleApplication=\"2\"",
        )
        .replace(
            "    <LexicalEntries>",
            "    <AffixTemplates><AffixTemplate requiredPartsOfSpeech=\"posV\"><Name>structural-tail\
template</Name><Slot morphologicalRules=\"mr5\"><Name>first-structural</Name></Slot><Slot \
morphologicalRules=\"mr1\"><Name>ordinary-middle</Name></Slot><Slot \
morphologicalRules=\"mr5\"><Name>later-structural</Name></Slot></AffixTemplate></AffixTemplates>\n    <LexicalEntries>",
        );
    let grammar = pg_grammar::load(&xml)
        .unwrap_or_else(|error| panic!("template tail fixture failed to load: {error}"));
    let structural = mrule_id_of(&grammar, "mr5");
    assert!(
        emit::composite_candidate_rules(&grammar)
            .structural_candidates
            .contains(&structural.0),
        "the counterexample must actually use the structural route on both sides"
    );
    assert_surface_recalled(&grammar, "fbfagg");
}
