//! Pins the compiled templated morphology relation with invented construct witnesses.

use pg_foma::analyzer::apply_up_against;
use pg_foma::structural_allomorph::{MarkerBinding, MarkerZone, MorphologyRelationResult};
use pg_foma::templated_compile::compile_templated_morphotactics;
use pg_grammar::model::{AllomorphId, Grammar, MorphRuleDef, OutputAction, TableId};
use std::collections::BTreeSet;

const MARKER_XML: &str = r#"
<HermitCrabInput><Language><Name>synthetic-templated-marker-union</Name>
  <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions>
    <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cs"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions></CharacterDefinitionTable>
  <NaturalClasses>
    <SegmentNaturalClass id="ncAny"><Name>Any</Name><Segment segment="ca"/><Segment segment="cb"/><Segment segment="cx"/><Segment segment="cz"/><Segment segment="cp"/><Segment segment="cs"/></SegmentNaturalClass>
    <SegmentNaturalClass id="ncB"><Name>B</Name><Segment segment="cb"/></SegmentNaturalClass>
    <SegmentNaturalClass id="ncX"><Name>X</Name><Segment segment="cx"/></SegmentNaturalClass>
  </NaturalClasses>
  <Strata><Stratum characterDefinitionTable="t" morphologicalRuleOrder="linear" morphologicalRules="mr" phonologicalRules="pr">
    <Name>marker-seam</Name>
    <PhonologicalRuleDefinitions><PhonologicalRule id="pr" multipleApplicationOrder="rightToLeftIterative">
      <Name>phonology-after-morphology</Name>
      <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncX"/></PhoneticSequence></PhoneticInput>
      <PhoneticOutput><PhoneticSequence><Segment segment="cz"/></PhoneticSequence></PhoneticOutput>
    </PhonologicalRule></PhonologicalRuleDefinitions>
    <MorphologicalRuleDefinitions><MorphologicalRule id="mr" requiredPartsOfSpeech="p" outputPartOfSpeech="p">
      <Name>marker-seam</Name><MorphologicalSubrules>
        <!-- Direct whole-root wrapper: this bypasses the marker union entirely. -->
        <MorphologicalSubrule id="wrapper"><MorphologicalInput><PhoneticSequence id="whole"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny"/></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index="whole"/><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
        <!-- Structural adjacent terminal drop: underlying ab -> ax, then phonology ax -> az. -->
        <MorphologicalSubrule id="drop"><MorphologicalInput><PhoneticSequence id="head"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="tail"><SimpleContext naturalClass="ncB"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="head"/><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
        <!-- A second marked recipe is required for marker A/B isolation: initial fixed-atom replacement. -->
        <MorphologicalSubrule id="initial"><MorphologicalInput><PhoneticSequence id="v0"><Segment segment="ca"/></PhoneticSequence><PhoneticSequence id="v1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index="v1"/></MorphologicalOutput></MorphologicalSubrule>
      </MorphologicalSubrules><MorphemeId>MORPH</MorphemeId>
    </MorphologicalRuleDefinitions>
    <AffixTemplates><AffixTemplate id="tpl" final="true" requiredPartsOfSpeech="p"><Name>tpl</Name><Slot optional="true" morphologicalRules="mr"><Name>slot</Name></Slot></AffixTemplate></AffixTemplates>
    <LexicalEntries><LexicalEntry id="root" partOfSpeech="p"><Allomorphs><Allomorph id="rootA"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs><MorphemeId>ROOT</MorphemeId></LexicalEntry></LexicalEntries>
  </Stratum></Strata>
</Language></HermitCrabInput>
"#;

fn load() -> Grammar {
    pg_grammar::load(MARKER_XML).unwrap_or_else(|e| panic!("marker fixture failed to load: {e}"))
}

fn technical_marker(id: AllomorphId) -> char {
    char::from_u32(0xF0000 + u32::from(id.0)).expect("classifier marker must be a scalar")
}

fn morph_allomorph_ids(g: &Grammar) -> Vec<AllomorphId> {
    match &g.mrules[0] {
        MorphRuleDef::AffixProcess(rule) => rule.allomorphs.iter().map(|a| a.id).collect(),
        other => panic!("marker fixture must contain an affix-process rule, got {other:?}"),
    }
}

fn assert_identity(
    relation: &impl MorphologyRelationProbe,
    input: &str,
    expected_outputs: &[&str],
) {
    match relation.apply(input) {
        MorphologyRelationResult::Identity {
            outputs,
            consumed_markers,
        } => {
            let expected = expected_outputs
                .iter()
                .map(|output| (*output).to_owned())
                .collect::<BTreeSet<_>>();
            assert_eq!(outputs, expected);
            assert_eq!(consumed_markers, 0);
        }
        other => panic!("marker-free input must use identity only, got {other:?}"),
    }
}

fn assert_recipe(
    relation: &impl MorphologyRelationProbe,
    input: &str,
    expected_shape: &str,
    expected_outputs: &[&str],
) {
    match relation.apply(input) {
        MorphologyRelationResult::Recipe {
            shape_id,
            outputs,
            consumed_markers,
            ..
        } => {
            assert_eq!(shape_id, expected_shape);
            let expected = expected_outputs
                .iter()
                .map(|output| (*output).to_owned())
                .collect::<BTreeSet<_>>();
            assert_eq!(outputs, expected);
            assert_eq!(
                consumed_markers, 1,
                "the selected branch consumes exactly one marker"
            );
        }
        other => panic!("known marker must select exactly one recipe, got {other:?}"),
    }
}

fn assert_rejected(relation: &impl MorphologyRelationProbe, input: &str, reason: &str) {
    match relation.apply(input) {
        MorphologyRelationResult::Rejected { reason_id, .. } => assert_eq!(reason_id, reason),
        other => panic!("invalid marker input must be rejected by the relation, got {other:?}"),
    }
}

/// Probes the exact intermediate relation retained by the production compile.
trait MorphologyRelationProbe {
    fn marker_binding_for(&self, allomorph: AllomorphId) -> Option<MarkerBinding>;
    fn marked_input(&self, allomorph: AllomorphId, base_tokens: &str) -> String;
    fn apply(&self, input: &str) -> MorphologyRelationResult;
}

impl MorphologyRelationProbe for pg_foma::structural_allomorph::CompiledMorphologyRelation {
    fn marker_binding_for(&self, allomorph: AllomorphId) -> Option<MarkerBinding> {
        self.marker_binding_for(allomorph)
    }

    fn marked_input(&self, allomorph: AllomorphId, base_tokens: &str) -> String {
        self.marked_input(allomorph, base_tokens)
    }

    fn apply(&self, input: &str) -> MorphologyRelationResult {
        self.apply(input)
    }
}

fn assert_binding(binding: &MarkerBinding, expected_zone: MarkerZone, label: &str) {
    assert_eq!(binding.zone, expected_zone, "{label} marker zone");
    assert!(
        (binding.symbol as u32) >= 0xF0000,
        "{label} binding must expose a technical marker symbol"
    );
}

fn assert_profile_is_complete(profile: &pg_foma::templated_compile::TemplatedCompileProfile) {
    assert!(
        profile.supported_recipe_count > 0,
        "at least one recipe must be supported"
    );
    assert!(
        profile.compiled_recipe_count > 0,
        "at least one recipe must be compiled"
    );
    assert!(
        profile.fired_recipe_count > 0,
        "the real relation must fire on the witness"
    );
    assert_eq!(
        profile.marker_allocations, 2,
        "only the two marked structural allomorphs allocate markers"
    );
    assert_eq!(
        profile.marker_consumptions, profile.marker_allocations,
        "every marker must be consumed once"
    );
    assert_eq!(
        profile.marker_leaks, 0,
        "no technical marker may survive the relation"
    );
    assert_eq!(
        profile.missing_marker_subtrees, 0,
        "no allocated marker may lack a relation subtree"
    );
    assert_eq!(
        profile.unsupported_count, 0,
        "the selected synthetic witness has no unsupported allomorph"
    );
    assert_eq!(
        profile.uncovered_count, 0,
        "the selected synthetic witness has no uncovered action"
    );
    assert!(
        profile.skipped_rules.is_empty(),
        "a skipped phonological rule cannot yield a trusted artifact"
    );
}

#[test]
fn compiled_marker_union_is_total_and_composed_before_phonology() {
    let g = load();
    let compiled = compile_templated_morphotactics(&g)
        .expect("the complete synthetic marker fixture must compile");
    assert_profile_is_complete(&compiled.profile);
    let ids = morph_allomorph_ids(&g);
    let relation = &compiled.morphology_relation;
    let binding_a = relation
        .marker_binding_for(ids[1])
        .expect("drop recipe marker binding");
    let binding_b = relation
        .marker_binding_for(ids[2])
        .expect("initial recipe marker binding");
    assert_binding(&binding_a, MarkerZone::Suffix, "terminal-drop");
    assert_binding(&binding_b, MarkerZone::Prefix, "initial-replacement");
    assert!(
        relation.marker_binding_for(ids[0]).is_none(),
        "direct wrapper must bypass marker allocation"
    );
    assert_ne!(
        binding_a.symbol, binding_b.symbol,
        "A/B recipes need isolated unique markers"
    );

    // Probe the union before later stages can hide an unsafe identity branch.
    assert_identity(relation, "ab", &["ab"]);
    let marked_a = relation.marked_input(ids[1], "ab");
    let marked_b = relation.marked_input(ids[2], "ab");
    assert_recipe(relation, &marked_a, "AdjacentTerminalDrop", &["ax"]);
    assert_recipe(
        relation,
        &marked_b,
        "AmharicInitialVowelReplacement",
        &["pb"],
    );
    let foreign = marked_a.replacen(binding_a.symbol, technical_marker(AllomorphId(0x7fff)), 1);
    assert_rejected(relation, &foreign, "foreign-marker");
    // Nest production-owned placement to create invalid multi-marker inputs without assuming a zone.
    let multiple = relation.marked_input(ids[1], &marked_b);
    let duplicate = relation.marked_input(ids[1], &marked_a);
    assert_rejected(relation, &multiple, "multiple-markers");
    assert_rejected(relation, &duplicate, "duplicate-marker");

    // Wrapper direct emission is proven at M, not inferred from a successful final lookup.
    assert!(relation.marker_binding_for(ids[0]).is_none());

    // Separately, the final network must retain marker-free identity and direct wrapper paths.
    let bare = compiled.proposer.propose("ab");
    assert!(
        !bare.is_empty(),
        "marker-free root identity must remain reachable"
    );
    let wrapped = compiled.proposer.propose("pabs");
    assert!(
        !wrapped.is_empty(),
        "direct whole-root wrapper must bypass structural markers"
    );

    // Morphology introduces x before phonology maps it to z.
    let realized = compiled.proposer.propose("az");
    assert!(
        !realized.is_empty(),
        "the marked terminal-drop recipe must fire before phonology"
    );
    assert!(
        compiled.proposer.propose("ax").is_empty(),
        "pre-phonology x must not be exposed as final surface"
    );

    // The finalized network must not leak markers through decoded or raw outputs.
    for marker_id in ids {
        let Some(binding) = relation.marker_binding_for(marker_id) else {
            continue;
        };
        let marker = binding.symbol;
        assert!(
            compiled
                .proposer
                .apply_up_raw("az")
                .into_iter()
                .all(|path| !path.contains(marker)),
            "marker {marker_id:?} must be consumed before the final proposer"
        );
    }
    assert!(
        apply_up_against(&compiled.network, "az")
            .into_iter()
            .all(|path| !path.chars().any(|ch| (ch as u32) >= 0xF0000)),
        "the compiled relation's final network must not leak technical markers"
    );
}

#[test]
fn foreign_unknown_and_multiple_markers_fail_closed_without_identity_fallback() {
    let g = load();
    let compiled = compile_templated_morphotactics(&g)
        .expect("the complete synthetic marker fixture must compile");
    let ids = morph_allomorph_ids(&g);
    let relation = &compiled.morphology_relation;
    let known_binding = relation
        .marker_binding_for(ids[1])
        .expect("known A marker binding");
    let other_known_binding = relation
        .marker_binding_for(ids[2])
        .expect("known B marker binding");
    assert_binding(&known_binding, MarkerZone::Suffix, "terminal-drop");
    assert_binding(
        &other_known_binding,
        MarkerZone::Prefix,
        "initial-replacement",
    );
    let known = relation.marked_input(ids[1], "ab");
    let other_known = relation.marked_input(ids[2], "ab");
    let foreign = known.replacen(
        known_binding.symbol,
        technical_marker(AllomorphId(0x7fff)),
        1,
    );

    // Probe invalid markers before later alphabet or cleanup stages can reject them for another reason.
    assert_rejected(relation, &foreign, "foreign-marker");
    assert_rejected(
        relation,
        &relation.marked_input(ids[1], &other_known),
        "multiple-markers",
    );
    assert_rejected(
        relation,
        &relation.marked_input(ids[1], &known),
        "duplicate-marker",
    );

    // Duplicate allocation is a construction error, so mutate IDs only after loading.
    let mut malformed = load();
    if let MorphRuleDef::AffixProcess(rule) = &mut malformed.mrules[0] {
        let first_marked = rule.allomorphs[1].id;
        rule.allomorphs[2].id = first_marked;
    }
    match compile_templated_morphotactics(&malformed) {
        Ok(_) => panic!("duplicate marker allocation must reject the complete artifact"),
        Err(error) => assert!(
            error.to_string().contains("marker") || error.to_string().contains("allomorph"),
            "duplicate marker rejection must name the marker/allomorph fault: {error}"
        ),
    }
}

#[test]
fn marker_consumption_is_exactly_once_and_never_cross_fires_between_allomorphs() {
    let g = load();
    let mut compiled = compile_templated_morphotactics(&g)
        .expect("the complete synthetic marker fixture must compile");
    let ids = morph_allomorph_ids(&g);
    assert!(
        ids.len() >= 2,
        "fixture must allocate independent allomorph identities"
    );
    let relation = &compiled.morphology_relation;
    let first_binding = relation
        .marker_binding_for(ids[1])
        .expect("known A marker binding");
    let second_binding = relation
        .marker_binding_for(ids[2])
        .expect("known B marker binding");
    assert_binding(&first_binding, MarkerZone::Suffix, "terminal-drop");
    assert_binding(&second_binding, MarkerZone::Prefix, "initial-replacement");
    assert_ne!(
        first_binding.symbol, second_binding.symbol,
        "each structural allomorph needs a unique marker"
    );

    // Branch-tagged observations prevent aggregate counters from hiding A/B cross-fire.
    assert_recipe(
        relation,
        &relation.marked_input(ids[1], "ab"),
        "AdjacentTerminalDrop",
        &["ax"],
    );
    assert_recipe(
        relation,
        &relation.marked_input(ids[2], "ab"),
        "AmharicInitialVowelReplacement",
        &["pb"],
    );
    assert_eq!(
        compiled.profile.marker_consumptions,
        compiled.profile.marker_allocations
    );
    assert_eq!(compiled.profile.marker_leaks, 0);
}

// Keep the model action vocabulary visible in this compile-time gate.
#[allow(dead_code)]
fn action_shape_is_not_a_role_label(
    action: OutputAction,
    table: TableId,
) -> (OutputAction, TableId) {
    (action, table)
}
