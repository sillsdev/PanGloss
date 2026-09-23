use pg_grammar::model::MorphRuleDef;

use super::*;
use crate::capability::rhs_has_true_reduplication;
use crate::plan::{FragmentSpec, PlanNodeKind, Provenance as PlanProvenance};

fn family(id: &str) -> BackendFamily {
    BackendFamily {
        id: id.to_owned(),
        version: REGISTRY_SCHEMA_VERSION,
        parameters: vec![Parameter {
            name: "mode".to_owned(),
            domain: vec!["a".to_owned()],
            depends_on: Vec::new(),
        }],
        applicability: Applicability::Always,
        search_policy: FamilySearchPolicy::AlwaysSearch,
        ordering: Vec::new(),
        provenance: Provenance {
            source: "synthetic".to_owned(),
            note: "test".to_owned(),
            attested: false,
        },
    }
}

fn minimal_grammar() -> Grammar {
    pg_grammar::load(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
<PhonologicalFeatureSystem><PhoneticFeature id="f"><Name>f</Name><PossibleValues><PhoneticValue id="v"><Name>v</Name></PhoneticValue></PossibleValues></PhoneticFeature></PhonologicalFeatureSystem>
<CharacterDefinitionTable id="t"><Name>T</Name><Encoding>IPA</Encoding><SegmentDefinitions><SegmentDefinition id="s"><Representation>a</Representation><FeatureValuePairs><FeatureValuePair feature="f" value="v"/></FeatureValuePairs></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable>
<Strata><Stratum characterDefinitionTable="t"><Name>S</Name><Lexicon><LexicalEntry id="e" partOfSpeech="p"><Name>e</Name><Allomorphs><Allomorph id="a"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></Lexicon></Stratum></Strata>
</Language></HermitCrabInput>"#,
    )
    .expect("synthetic grammar")
}

fn baseline() -> Plan {
    let mut plan = Plan::new();
    let root = plan.add_node(PlanNodeKind::Leaf {
        fragment: FragmentSpec::CompositeEmissionMarker,
        provenance: PlanProvenance::CompositeEmission,
    });
    plan.set_root(root);
    plan
}

/// The baseline fact is derived from the candidate (Identity plus a plan-interpreting adapter), never from position -- rejected alternatives were a positional `i == 0` rule and a caller-supplied `&[bool]`.
#[test]
fn the_baseline_role_follows_the_baseline_plan_and_never_the_position() {
    let grammar = minimal_grammar();
    let base = baseline();
    let registry = Registry::seeded();
    let materialized = registry
        .materialize_distinct(&MaterializerContext {
            grammar: &grammar,
            baseline: &base,
        })
        .expect("the seeded registry must materialize for this grammar");
    assert!(
        materialized.len() >= 2,
        "this assertion needs at least a baseline and one non-baseline candidate to distinguish"
    );

    let baselines: Vec<&BackendInstance> = materialized
        .iter()
        .filter(|(_, candidate)| candidate.is_baseline())
        .map(|(instance, _)| instance)
        .collect();
    assert_eq!(
        baselines.len(),
        1,
        "exactly one candidate may be this grammar's default compilation; got {baselines:?}"
    );

    for (instance, candidate) in &materialized {
        if candidate.is_baseline() {
            assert_eq!(
                candidate.plan.root(),
                base.root(),
                "{}: a candidate marked Baseline must carry the baseline plan VERBATIM, or the \
                 role is a label rather than a derived fact",
                instance.family_id
            );
            assert!(
                backend_for(candidate.adapter).interprets_plan(),
                "{}: only the plan-interpreting adapter can be a plan's own compilation",
                instance.family_id
            );
        } else {
            let rewrites_the_plan = candidate.plan.root() != base.root();
            let different_compiler = !backend_for(candidate.adapter).interprets_plan();
            assert!(
                rewrites_the_plan || different_compiler,
                "{}: an Alternative must differ from the baseline in its PLAN or its COMPILER; a \
                 candidate that differs in neither is the baseline wearing a second label",
                instance.family_id
            );
        }
    }
}

#[test]
fn seeded_registry_has_stable_ids_and_metadata() {
    let registry = Registry::seeded();
    assert_eq!(
        registry
            .families()
            .map(|f| f.id.as_str())
            .collect::<BTreeSet<_>>(),
        SEEDED_FAMILIES.iter().copied().collect()
    );
    assert_eq!(
        registry.canonical_json(),
        Registry::seeded().canonical_json()
    );
    assert!(registry
        .families()
        .all(|family| !family.ordering.is_empty()));
}

#[test]
fn rejects_schema_version_cycle_domain_and_unknown_instance() {
    assert!(matches!(
        Registry::new(9),
        Err(RegistryError::UnsupportedSchema(9))
    ));
    let mut cyclic = family("cycle");
    cyclic.parameters = vec![
        Parameter {
            name: "a".to_owned(),
            domain: vec!["x".to_owned()],
            depends_on: vec!["b".to_owned()],
        },
        Parameter {
            name: "b".to_owned(),
            domain: vec!["x".to_owned()],
            depends_on: vec!["a".to_owned()],
        },
    ];
    assert!(matches!(
        Registry::new(1).unwrap().validate_family(&cyclic),
        Err(RegistryError::CyclicDependency(_))
    ));
    let mut empty = family("empty");
    empty.parameters[0].domain.clear();
    assert!(matches!(
        Registry::new(1).unwrap().validate_family(&empty),
        Err(RegistryError::EmptyDomain { .. })
    ));
    assert!(matches!(
        Registry::new(1)
            .unwrap()
            .validate_instance(&BackendInstance {
                family_id: "missing".to_owned(),
                parameters: BTreeMap::new()
            }),
        Err(RegistryError::UnknownFamily(_))
    ));
}

#[test]
fn loaded_metadata_requires_explicit_executable_materializer() {
    let seeded = Registry::seeded();
    let loaded = Registry::load_json(&seeded.canonical_json()).expect("valid metadata");
    assert!(matches!(
        loaded.validate_ready(),
        Err(RegistryError::MissingMaterializer(_))
    ));
    let grammar = minimal_grammar();
    let base = baseline();
    let instance = loaded.instances_for_grammar(&grammar).remove(0);
    assert!(matches!(
        loaded.materialize(
            &instance,
            &MaterializerContext {
                grammar: &grammar,
                baseline: &base
            }
        ),
        Err(MaterializeError::MissingMaterializer(_))
    ));
}

#[test]
fn extension_and_content_address_dedup_do_not_change_registry_core() {
    let grammar = minimal_grammar();
    let base = baseline();
    let mut registry = Registry::new(1).unwrap();
    let make = |label: &'static str| {
        Box::new(move |_i: &BackendInstance, c: &MaterializerContext<'_>| {
            Ok(LoweredCandidate {
                label,
                plan: c.baseline.clone(),
                adapter: EmissionStrategy::PlanComposed,
                role: CandidateRole::Alternative,
            })
        }) as MaterializerFn
    };
    registry
        .register_family(family("extension-a"), make("extension-a"))
        .unwrap();
    registry
        .register_family(family("extension-b"), make("extension-b"))
        .unwrap();
    let materialized = registry
        .materialize_distinct(&MaterializerContext {
            grammar: &grammar,
            baseline: &base,
        })
        .unwrap();
    assert_eq!(
        materialized.len(),
        1,
        "identical Plans from different families deduplicate by root NodeId"
    );
}
#[test]
fn policy_exclusions_happen_before_materialization_and_opt_in_restores_instances() {
    let grammar = minimal_grammar();
    let mut registry = Registry::new(REGISTRY_SCHEMA_VERSION).unwrap();
    let mut tie_family = family("synthetic-tie");
    tie_family.search_policy = FamilySearchPolicy::SkipOnCompositionalTopology;
    registry
        .register_family(
            tie_family,
            Box::new(
                |_instance: &BackendInstance, context: &MaterializerContext<'_>| {
                    Ok(LoweredCandidate {
                        label: "synthetic-tie",
                        plan: context.baseline.clone(),
                        adapter: EmissionStrategy::PlanComposed,
                        role: CandidateRole::Alternative,
                    })
                },
            ),
        )
        .unwrap();

    let (default_instances, declared_not_searched) =
        registry.instances_for_search(&grammar, true, false);
    assert!(default_instances.is_empty());
    assert_eq!(declared_not_searched, 1);

    let (all_instances, opt_in_declared_not_searched) =
        registry.instances_for_search(&grammar, true, true);
    assert_eq!(all_instances.len(), 1);
    assert_eq!(opt_in_declared_not_searched, 0);
}

#[test]
fn seeded_tie_policy_names_exactly_the_recorded_plan_rewrite_families() {
    let registry = Registry::seeded();
    let actual = registry
        .families()
        .filter(|family| family.search_policy == FamilySearchPolicy::SkipOnCompositionalTopology)
        .map(|family| family.id.as_str())
        .collect::<BTreeSet<_>>();
    let expected = [
        FAMILY_CLASS_EXCEPTION_CASCADE,
        FAMILY_COMPLETE_TEMPLATE,
        FAMILY_SPECIALIZED_BRANCH,
        FAMILY_COPY_BRANCH,
        FAMILY_LAYERED_MORPHOLOGY,
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected);
}

// Cross-derivation agreement: each fact below has more than one derivation in this crate, so these tests assert them all against the same synthetic grammar to keep them from drifting apart.

/// A `PhonologicalSubrule` gated purely on `requiredPartsOfSpeech`, in a grammar declaring no MPR features at all.
fn pos_gated_no_mpr_features_grammar() -> Grammar {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>PosGatedNoMprFixture</Name>
<PartsOfSpeech>
  <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
  <PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech>
</PartsOfSpeech>
<CharacterDefinitionTable id="t1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<PhonologicalRuleDefinitions>
  <PhonologicalRule id="prule1">
    <Name>posGate</Name>
    <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
    <PhonologicalSubrules>
      <PhonologicalSubrule requiredPartsOfSpeech="posV">
        <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
      </PhonologicalSubrule>
    </PhonologicalSubrules>
  </PhonologicalRule>
</PhonologicalRuleDefinitions>
<Strata>
  <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prule1">
    <Name>S</Name>
    <LexicalEntries>
      <LexicalEntry id="e1" partOfSpeech="posV">
        <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
        <Gloss>E1</Gloss>
      </LexicalEntry>
      <LexicalEntry id="e2" partOfSpeech="posN">
        <Allomorphs><Allomorph id="a2"><PhoneticShape>q</PhoneticShape></Allomorph></Allomorphs>
        <Gloss>E2</Gloss>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>
"#;
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// `HasGatedExceptions` must track `gate::find_gated_subrules` with no extra `mpr_features` precondition, agreeing with `backend_space::GrammarFacts` and the offered family.
#[test]
fn pos_only_gating_without_mpr_features_agrees_across_every_derivation() {
    let g = pos_gated_no_mpr_features_grammar();
    assert!(
        g.mpr_features.is_empty(),
        "fixture premise: this grammar declares no MPR features at all"
    );

    // (a) the real mechanism
    let gated = crate::gate::find_gated_subrules(&g, &crate::enumerate::prules_in_order(&g));
    assert_eq!(
        gated.len(),
        1,
        "gate::find_gated_subrules must see the requiredPartsOfSpeech-gated subrule"
    );
    assert_eq!(
        crate::backend_space::GrammarFacts::from_grammar(&g).gated_subrules,
        1,
        "backend_space projects the same mechanism and must report the same count"
    );

    // (b) the registry predicate
    assert!(
        Applicability::HasGatedExceptions.matches(&g),
        "HasGatedExceptions must be a projection of gate::find_gated_subrules, not a \
         reimplementation with an mpr_features precondition of its own"
    );

    // (c) the family the predicate gates
    let offered = Registry::seeded()
        .instances_for_grammar(&g)
        .into_iter()
        .map(|instance| instance.family_id)
        .collect::<BTreeSet<_>>();
    assert!(
        offered.contains(FAMILY_CLASS_EXCEPTION_CASCADE),
        "a genuinely gated grammar must be offered {FAMILY_CLASS_EXCEPTION_CASCADE}; offered: \
         {offered:?}"
    );
}

/// One `MorphologicalRule` with a non-default `redupMorphType` whose RHS copies the input part once (not reduplication); `echoed` copies it twice, which is.
fn redup_hint_grammar(echoed: bool) -> Grammar {
    let copies = if echoed {
        r#"<CopyFromInput index="stem" /><CopyFromInput index="stem" />"#
    } else {
        r#"<CopyFromInput index="stem" /><InsertSegments><PhoneticShape>q</PhoneticShape></InsertSegments>"#
    };
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>RedupHintFixture</Name>
<PartsOfSpeech>
  <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
</PartsOfSpeech>
<CharacterDefinitionTable id="t1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses>
  <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="c1" /><Segment segment="c2" /></SegmentNaturalClass>
</NaturalClasses>
<Strata>
  <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mr1">
    <Name>S</Name>
    <MorphologicalRuleDefinitions>
      <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
        <Name>redupish</Name>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="sub1">
            <MorphologicalInput>
              <PhoneticSequence id="stem">
                <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
              </PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput redupMorphType="prefix">{copies}</MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
        <Gloss>RED</Gloss>
      </MorphologicalRule>
    </MorphologicalRuleDefinitions>
    <LexicalEntries>
      <LexicalEntry id="e1" partOfSpeech="posV">
        <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
        <Gloss>E1</Gloss>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>
"#
    );
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// `Applicability::HasReduplication` and `backend_space::reduplication_count` must both consume `rhs_has_true_reduplication`, never a merely non-`Implicit` `redup_hint`.
#[test]
fn non_default_redup_hint_without_an_echoed_part_is_reduplication_free_everywhere() {
    let g = redup_hint_grammar(false);
    let allomorph = match &g.mrules[0] {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs[0],
        other => panic!("fixture premise: expected an AffixProcess rule, got {other:?}"),
    };
    assert_ne!(
        allomorph.redup_hint,
        pg_grammar::model::ReduplicationHint::Implicit,
        "fixture premise: the hint must be non-default"
    );

    // (a) the authority
    assert!(
        !rhs_has_true_reduplication(&allomorph.rhs),
        "no input part is echoed twice, so this is not reduplication"
    );
    // (b) the registry predicate
    assert!(
        !Applicability::HasReduplication.matches(&g),
        "HasReduplication must consume rhs_has_true_reduplication, not the redup_hint"
    );
    // (c) the backend-space count
    assert_eq!(
        crate::backend_space::GrammarFacts::from_grammar(&g).reduplicative_allomorphs,
        0,
        "reduplication_count must consume rhs_has_true_reduplication, not the redup_hint"
    );
    // (d) the family the predicate gates
    let offered = Registry::seeded()
        .instances_for_grammar(&g)
        .into_iter()
        .map(|instance| instance.family_id)
        .collect::<BTreeSet<_>>();
    assert!(
        !offered.contains(FAMILY_COPY_BRANCH),
        "a reduplication-free grammar must not be offered {FAMILY_COPY_BRANCH}; offered: \
         {offered:?}"
    );
}

/// The other half of the same contract: a genuinely reduplicating grammar must still be detected by all three derivations.
#[test]
fn a_genuinely_echoed_part_is_reduplication_everywhere() {
    let g = redup_hint_grammar(true);
    let allomorph = match &g.mrules[0] {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs[0],
        other => panic!("fixture premise: expected an AffixProcess rule, got {other:?}"),
    };

    assert!(rhs_has_true_reduplication(&allomorph.rhs));
    assert!(Applicability::HasReduplication.matches(&g));
    assert_eq!(
        crate::backend_space::GrammarFacts::from_grammar(&g).reduplicative_allomorphs,
        1
    );
    let offered = Registry::seeded()
        .instances_for_grammar(&g)
        .into_iter()
        .map(|instance| instance.family_id)
        .collect::<BTreeSet<_>>();
    assert!(
        offered.contains(FAMILY_COPY_BRANCH),
        "a genuinely reduplicating grammar must be offered {FAMILY_COPY_BRANCH}; offered: \
         {offered:?}"
    );
}
