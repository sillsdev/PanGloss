//! Task 7.5 of `openspec/changes/cleanup-and-recipe-parity`, gated from OUTSIDE the crate.
//!
//! An integration test is the right place for this one, not a unit test: half of what task 7.5
//! claims is a claim about the crate's PUBLIC surface -- that
//! [`pg_foma::recipe_registry::Registry::executable_candidate`] is the only way to obtain an
//! [`pg_foma::executable_candidate::ExecutableCandidate`], that the type carries no `Deserialize`
//! back door, and that a portable Plan document survives leaving the process. None of that can be
//! observed from inside `src/`.
//!
//! Two things this file deliberately does NOT do. It does not evaluate, rank, or select anything:
//! Wave 3 measured a candidate that was 2.2x cheaper than the winner and returned a DIFFERENT
//! analysis set, so ranking is only legitimate downstream of the measured parity relation, and
//! nothing constructed here has been measured against an oracle. And it does not assert any
//! particular compiler is best -- the winners moved this morning
//! (`plan-composed` now wins nowhere), so a test that pinned one would be pinning a fact with a
//! shelf life.

use std::collections::BTreeSet;

use pg_foma::executable_candidate::{
    CandidateConstructionError, LoweringAdapter, PortableFragment, PortableNodeKind, PortablePlan,
    PortablePlanError, RuntimeRequirement,
};
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::plan::{FragmentSpec, Plan, PlanNodeKind, Provenance};
use pg_foma::recipe_registry::{
    MaterializerContext, RecipeInstance, Registry, FAMILY_ORDERED_MORPHOPHONOLOGY,
    FAMILY_SURFACE_PROBE_MORPHOLOGY,
};
use pg_grammar::model::{Grammar, MorphRuleDef};

/// A minimal grammar whose ONLY morphological rule is a `RealizationalRule` -- reused verbatim from
/// `tests/strategy_aware_capability_gate.rs`, which in turn reuses `capability.rs`'s own fixture, so
/// this file is not litigating a third, differently-shaped grammar's characterization.
///
/// This shape is the whole point of the paired tests below. `strategy_coverage` records
/// `RealizationalMorphology` as `CannotRepresent` for `PlanComposed` and `Represents` for both
/// whole-grammar compilers, with `uflexc`'s own `skipped` / `continue` as the citation -- so the
/// SAME grammar's SAME mechanism must be refused under one adapter and sealed under another. A
/// verdict that could not tell those apart is the `uflexc`/`Compounding` inheritance bug.
const REALIZATIONAL_XML: &str = r#"<HermitCrabInput><Language><Name>RealizAlone</Name>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1">
      <Name>S</Name>
      <MorphologicalRuleDefinitions>
        <RealizationalRule id="rr1">
          <Name>Realiz</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="sub1">
              <MorphologicalInput><PhoneticSequence id="s0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><CopyFromInput index="s0" /></MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
        </RealizationalRule>
      </MorphologicalRuleDefinitions>
      <LexicalEntries>
        <LexicalEntry id="e1">
          <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

/// The same grammar with the realizational rule removed -- the negative control, likewise reused
/// from `tests/strategy_aware_capability_gate.rs`. Nothing here is strategy-conditional.
const NO_REALIZATIONAL_XML: &str = r#"<HermitCrabInput><Language><Name>PlainAlone</Name>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1">
      <Name>S</Name>
      <LexicalEntries>
        <LexicalEntry id="e1">
          <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
        </LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// A hand-built baseline plan, the same shape `recipe_registry`'s own tests use. Every family
/// exercised here is `SafeTransform::Identity`, so this plan IS each candidate's plan -- which
/// keeps the test about candidate construction rather than about plan rewriting.
fn baseline_plan() -> Plan {
    let mut plan = Plan::new();
    let leaf = plan.add_node(PlanNodeKind::Leaf {
        fragment: FragmentSpec::LexiconFragment { entries: None },
        provenance: Provenance::Lexicon,
    });
    plan.set_root(leaf);
    plan
}

fn instance_for(registry: &Registry, grammar: &Grammar, family: &str) -> RecipeInstance {
    registry
        .instances_for_grammar(grammar)
        .into_iter()
        .find(|instance| instance.family_id == family)
        .unwrap_or_else(|| panic!("{family} must be offered to this fixture"))
}

// =================================================================================================
// The typed refusal, and the fact that it is a refusal about ONE adapter
// =================================================================================================

/// THE TEST THIS TASK EXISTS FOR. `uflexc` -- the only lexicon emitter `PlanComposed` has -- emits
/// no lexc line at all for a `RealizationalRule`, so its proposer returns zero candidates for any
/// word requiring one. Construction must therefore REFUSE, with a typed error naming the mechanism,
/// the adapter, and `strategy_coverage`'s own citation.
///
/// What must NOT happen is the thing this task forbids in as many words: quietly sealing the
/// candidate against a compiler that happens to work. The measured cost of that shape is on record
/// twice -- `Compounding` rested at `ConfirmOnly` grammar-wide while `uflexc` could not propose a
/// single compound, and Amharic's 2.2x-cheaper candidate returned a different analysis set. A
/// substitution made here would be indistinguishable downstream from a measurement of the candidate
/// that was actually asked for.
#[test]
fn a_construct_the_adapter_cannot_represent_is_a_typed_refusal_never_a_substitution() {
    let g = load(REALIZATIONAL_XML);
    assert!(
        matches!(g.mrules[0], MorphRuleDef::Realizational(_)),
        "fixture premise: this grammar's only morphological rule is a RealizationalRule"
    );
    let registry = Registry::seeded();
    let semantics = GrammarSemantics::derive(&g);
    let baseline = baseline_plan();
    let context = MaterializerContext {
        grammar: &g,
        baseline: &baseline,
    };
    let instance = instance_for(&registry, &g, FAMILY_ORDERED_MORPHOPHONOLOGY);

    match registry.executable_candidate(&instance, &context, &semantics) {
        Err(CandidateConstructionError::MechanismRefused {
            adapter,
            mechanisms,
            citations,
            ..
        }) => {
            assert_eq!(
                adapter,
                LoweringAdapter::ControllablePlanCompose,
                "the refusal must name WHOSE refusal it is"
            );
            assert!(
                !mechanisms.is_empty(),
                "a refusal must name at least one mechanism"
            );
            assert!(
                citations
                    .iter()
                    .any(|c| c.representation == "cannot-represent"),
                "the refusal must carry strategy_coverage's own citation, got {citations:?}"
            );
        }
        other => panic!(
            "a CannotRepresent construct must produce a typed refusal, never a fallback; got \
             {other:?}"
        ),
    }
}

/// The other half, and the half that makes the refusal above a fact about an ADAPTER rather than a
/// blanket failure: the SAME grammar, the SAME mechanism, sealed successfully under a whole-grammar
/// adapter that can represent it. If this failed too, the refusal above would be indistinguishable
/// from "this fixture is broken".
#[test]
fn the_same_grammar_seals_under_an_adapter_that_can_represent_the_construct() {
    let g = load(REALIZATIONAL_XML);
    let registry = Registry::seeded();
    let semantics = GrammarSemantics::derive(&g);
    let baseline = baseline_plan();
    let context = MaterializerContext {
        grammar: &g,
        baseline: &baseline,
    };
    let instance = instance_for(&registry, &g, FAMILY_SURFACE_PROBE_MORPHOLOGY);

    let candidate = registry
        .executable_candidate(&instance, &context, &semantics)
        .expect("the tuned surface adapter represents RealizationalMorphology");

    assert_eq!(candidate.adapter(), LoweringAdapter::TunedSurfaceEmit);
    assert_eq!(
        candidate.certification_scope().adapter(),
        LoweringAdapter::TunedSurfaceEmit,
        "a certification scope that cannot name whose scope it is, is the bug this replaces"
    );
    assert!(
        !candidate.mechanism_bindings().is_empty(),
        "this grammar declares a morphological rule, so at least one mechanism must be bound"
    );
    for binding in candidate.mechanism_bindings() {
        assert_eq!(
            binding.strategy(),
            candidate.adapter().strategy(),
            "every binding must name the candidate's own compiler"
        );
    }
    let scoped: usize = candidate.certification_scope().exact_fst().len()
        + candidate.certification_scope().confirm_only().len()
        + candidate.certification_scope().peeled().len();
    assert_eq!(
        scoped,
        candidate.mechanism_bindings().len(),
        "every bound mechanism must appear in the scope exactly once -- a sealed candidate has no \
         Refused bindings left"
    );
}

// =================================================================================================
// What a sealed candidate binds
// =================================================================================================

#[test]
fn a_sealed_candidate_binds_digests_document_adapter_requirements_and_scope() {
    let g = load(NO_REALIZATIONAL_XML);
    let registry = Registry::seeded();
    let semantics = GrammarSemantics::derive(&g);
    let baseline = baseline_plan();
    let context = MaterializerContext {
        grammar: &g,
        baseline: &baseline,
    };
    let instance = instance_for(&registry, &g, FAMILY_ORDERED_MORPHOPHONOLOGY);
    let candidate = registry
        .executable_candidate(&instance, &context, &semantics)
        .expect("a plain grammar's baseline must seal");

    assert_eq!(candidate.family_id(), FAMILY_ORDERED_MORPHOPHONOLOGY);
    assert_eq!(candidate.instance(), &instance);
    assert_eq!(candidate.adapter(), LoweringAdapter::ControllablePlanCompose);
    assert!(candidate.adapter().interprets_plan());

    // Both digests are real SHA-256, and the plan digest is the document's own -- never the plan's
    // 64-bit FNV root, which `plan.rs` itself documents as not collision-resistant.
    for digest in [candidate.plan_digest(), candidate.semantic_digest()] {
        assert!(digest.starts_with("sha256:"), "{digest}");
        assert_eq!(digest.len(), "sha256:".len() + 64, "{digest}");
    }
    assert_eq!(candidate.plan_digest(), candidate.plan_document().digest());
    assert_ne!(
        candidate.plan_digest(),
        candidate.semantic_digest(),
        "two projections over different artifacts must not collide"
    );

    // The runtime requirements are DERIVED from the adapter and the plan, not declared.
    let requirements: BTreeSet<&RuntimeRequirement> =
        candidate.runtime_requirements().iter().collect();
    assert!(requirements.contains(&RuntimeRequirement::RootedPlan));
    assert!(requirements.contains(&RuntimeRequirement::PlanInterpretedByAdapter));
    assert!(
        !requirements.contains(&RuntimeRequirement::WholeGrammarRecompilation),
        "the plan-interpreting adapter must not claim whole-grammar recompilation"
    );
    assert!(
        candidate.runtime_requirements().iter().any(|r| matches!(
            r,
            RuntimeRequirement::ControllableSubtreeBuildable { .. }
        )),
        "the interpreting adapter must record what build_controllable can and cannot build"
    );
    assert!(candidate.mechanism_graph().validate().is_ok());
}

/// A whole-grammar adapter gets the complementary requirement set: it recompiles the grammar its own
/// way and never reads the plan, which is exactly why `recipe_runtime::build_candidate` refuses to
/// be handed one ("evaluating this permutation there would measure the baseline network and report
/// it as this permutation").
#[test]
fn a_whole_grammar_adapter_records_that_it_ignores_the_plan() {
    let g = load(NO_REALIZATIONAL_XML);
    let registry = Registry::seeded();
    let semantics = GrammarSemantics::derive(&g);
    let baseline = baseline_plan();
    let context = MaterializerContext {
        grammar: &g,
        baseline: &baseline,
    };
    let candidate = registry
        .executable_candidate(
            &instance_for(&registry, &g, FAMILY_SURFACE_PROBE_MORPHOLOGY),
            &context,
            &semantics,
        )
        .expect("the whole-grammar baseline must seal");

    assert!(!candidate.adapter().interprets_plan());
    let requirements: BTreeSet<&RuntimeRequirement> =
        candidate.runtime_requirements().iter().collect();
    assert!(requirements.contains(&RuntimeRequirement::WholeGrammarRecompilation));
    assert!(!requirements.contains(&RuntimeRequirement::PlanInterpretedByAdapter));
}

/// The semantic digest identifies the GRAMMAR's semantics, so it is stable across two independent
/// loads of the same source and moves when the grammar does. Without both halves it would be either
/// a per-process nonce or a constant.
#[test]
fn the_semantic_digest_is_stable_across_loads_and_moves_with_the_grammar() {
    let registry = Registry::seeded();
    let baseline = baseline_plan();

    let digest_of = |xml: &str| {
        let g = load(xml);
        let semantics = GrammarSemantics::derive(&g);
        let context = MaterializerContext {
            grammar: &g,
            baseline: &baseline,
        };
        registry
            .executable_candidate(
                &instance_for(&registry, &g, FAMILY_SURFACE_PROBE_MORPHOLOGY),
                &context,
                &semantics,
            )
            .expect("must seal")
            .semantic_digest()
            .to_owned()
    };

    assert_eq!(digest_of(NO_REALIZATIONAL_XML), digest_of(NO_REALIZATIONAL_XML));
    assert_ne!(digest_of(NO_REALIZATIONAL_XML), digest_of(REALIZATIONAL_XML));
}

// =================================================================================================
// The portable document: round-trip, and refusal of corruption
// =================================================================================================

/// The required round-trip property, exercised on a document a SEALED candidate actually carries
/// (not a hand-built one): JSON out, JSON in, decode to a `Plan`, re-encode -- identical document,
/// identical digest.
#[test]
fn a_sealed_candidates_plan_document_round_trips_with_its_digest_intact() {
    let g = load(NO_REALIZATIONAL_XML);
    let registry = Registry::seeded();
    let semantics = GrammarSemantics::derive(&g);
    let baseline = baseline_plan();
    let context = MaterializerContext {
        grammar: &g,
        baseline: &baseline,
    };
    let candidate = registry
        .executable_candidate(
            &instance_for(&registry, &g, FAMILY_ORDERED_MORPHOPHONOLOGY),
            &context,
            &semantics,
        )
        .expect("must seal");

    let document = candidate.plan_document();
    let json = document.canonical_json();
    let parsed = PortablePlan::from_json(&json).expect("a candidate's document must parse");
    assert_eq!(&parsed, document);
    assert_eq!(parsed.digest(), candidate.plan_digest());

    let rebuilt = parsed.decode().expect("a candidate's document must decode");
    assert_eq!(rebuilt.root(), baseline.root());
    let reencoded = PortablePlan::encode(&rebuilt);
    assert_eq!(&reencoded, document);
    assert_eq!(reencoded.digest(), candidate.plan_digest());
}

/// Corruption is REFUSED, not repaired -- from outside the crate, on a document a sealed candidate
/// handed out. The decoder recomputes every content address from the decoded content, so editing a
/// payload while leaving its declared id alone cannot be absorbed.
#[test]
fn a_corrupted_plan_document_is_refused_rather_than_repaired() {
    let g = load(NO_REALIZATIONAL_XML);
    let registry = Registry::seeded();
    let semantics = GrammarSemantics::derive(&g);
    let baseline = baseline_plan();
    let context = MaterializerContext {
        grammar: &g,
        baseline: &baseline,
    };
    let candidate = registry
        .executable_candidate(
            &instance_for(&registry, &g, FAMILY_ORDERED_MORPHOPHONOLOGY),
            &context,
            &semantics,
        )
        .expect("must seal");

    let mut tampered = candidate.plan_document().clone();
    let mut edited = false;
    for node in &mut tampered.nodes {
        if let PortableNodeKind::Leaf { fragment, .. } = &mut node.node {
            *fragment = PortableFragment::CompositeEmissionMarker;
            edited = true;
        }
    }
    assert!(edited, "the fixture plan must contain a leaf to tamper with");
    assert_ne!(
        tampered.digest(),
        candidate.plan_digest(),
        "a tampered document must not digest like the original"
    );
    assert!(
        matches!(
            tampered.decode(),
            Err(PortablePlanError::ContentAddressMismatch { .. })
        ),
        "a payload edit under an unchanged declared id must be refused"
    );
}

// =================================================================================================
// Nothing about routing moved
// =================================================================================================

/// This task is a construction/validation rework, so the set of instances a grammar is OFFERED, and
/// the set of candidates that materialize, must be exactly what they were. In particular a sealing
/// refusal must not remove anything from either: the refusing family is still offered and still
/// materializes, it simply cannot be SEALED as an executable candidate.
#[test]
fn sealing_refusals_change_neither_the_offered_instances_nor_the_materialized_candidates() {
    let g = load(REALIZATIONAL_XML);
    let registry = Registry::seeded();
    let baseline = baseline_plan();
    let context = MaterializerContext {
        grammar: &g,
        baseline: &baseline,
    };

    let offered: BTreeSet<String> = registry
        .instances_for_grammar(&g)
        .into_iter()
        .map(|instance| instance.family_id)
        .collect();
    assert!(
        offered.contains(FAMILY_ORDERED_MORPHOPHONOLOGY),
        "the family whose adapter refuses must still be OFFERED -- routing is untouched"
    );
    assert!(offered.contains(FAMILY_SURFACE_PROBE_MORPHOLOGY));

    let materialized = registry
        .materialize_distinct(&context)
        .expect("materialization must be unaffected by candidate sealing");
    assert!(
        materialized
            .iter()
            .any(|(instance, _)| instance.family_id == FAMILY_ORDERED_MORPHOPHONOLOGY),
        "the refusing family must still materialize a CandidatePlan -- only sealing refuses"
    );
}
