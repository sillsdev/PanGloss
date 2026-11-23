//! Typed recall refusals and construct-removal controls, separate from the single-test scoreboard census.
use pg_conformance_fixtures::discover;
use pg_foma::enumerate::EmissionStrategy;
use pg_health::capability::{CapabilityRefusal, ProposalRecallGap};

const RECALL_REGRESSIONS: &[(&str, ProposalRecallGap)] = &[
    (
        "machine:edge-cases/alias-output-0-literal",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/alias-output-0-singleton",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/alias-output-1-literal",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/alias-output-1-singleton",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/partial-inherited-output-literal",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/partial-inherited-output-singleton",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/partial-inherited-output-feature",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/partial-feature-literal-rewrite",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/featureless-literal-target",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/featureless-singleton-target",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/featureless-unused-feature",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/partial-feature-literal-output",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/partial-feature-singleton-rewrite",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/partial-rule-deleted-material",
        ProposalRecallGap::PartialRuleDeletionOrReduplication,
    ),
    (
        "machine:edge-cases/partial-rule-wrapped-material",
        ProposalRecallGap::PartialRuleDeletionOrReduplication,
    ),
    (
        "machine:edge-cases/class-deletion-reinsertion-strrep",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/clitic-rule-designation",
        ProposalRecallGap::TemplateReentry,
    ),
    (
        "machine:edge-cases/copy-agreement-featureless-redup",
        ProposalRecallGap::PhonologicalReduplication,
    ),
    (
        "machine:edge-cases/final-template-merge-key",
        ProposalRecallGap::TemplateReentry,
    ),
    (
        "machine:edge-cases/final-template-partial-discriminators",
        ProposalRecallGap::TemplateReentry,
    ),
    (
        "machine:edge-cases/final-template-partial-trace",
        ProposalRecallGap::TemplateReentry,
    ),
    (
        "machine:edge-cases/truncate-morphotactic-copy-u",
        ProposalRecallGap::TruncationComposition,
    ),
    (
        "machine:languages/metathesis-phase-isolation",
        ProposalRecallGap::PhonologicalReduplication,
    ),
    (
        "staging:edge-cases/backend-ordered-generic",
        ProposalRecallGap::PhonologicalReduplication,
    ),
    (
        "staging:edge-cases/clitic-rule-designation-iscliticrule",
        ProposalRecallGap::TemplateReentry,
    ),
    (
        "staging:edge-cases/featureless-segment-class-rewrite",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/generated-full-intermediate",
        ProposalRecallGap::NarrowingClassOutput,
    ),
    (
        "staging:edge-cases/rewrite-default-epenthesis",
        ProposalRecallGap::SymbolicEpenthesis,
    ),
    (
        "staging:edge-cases/rewrite-default-full-set",
        ProposalRecallGap::SymbolicEpenthesis,
    ),
];

#[test]
fn no_phonology_partial_feature_spellings_recover_complete_identities() {
    let fixture = discover()
        .into_iter()
        .find(|f| f.label() == "machine:edge-cases/partial-feature-no-rewrite")
        .expect("partial feature fixture");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    let mut words = Vec::new();
    for a in ["k", "t", "s", "z", "a", "i"] {
        for b in ["k", "t", "s", "z", "a", "i"] {
            for c in ["k", "t", "s", "z", "a", "i"] {
                words.push(format!("{a}{b}{c}"));
            }
        }
    }
    words.extend(["aa".into(), "aaaa".into()]);
    let row = pg_foma_backend::scoreboard::measure("partial feature exhaustive", &grammar, &words);
    assert!(
        matches!(
            row.cells[0].outcome,
            pg_foma_backend::scoreboard::CellOutcome::OracleExact
        ),
        "{:?}",
        row.cells[0]
    );
}

const COLLATERAL_REFUSALS: &[(&str, ProposalRecallGap)] = &[
    (
        "staging:edge-cases/featureless-single-member-class-rewrite",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/ambiguous-disagree-feature-class-bounded-rtl",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/ambiguous-disagree-feature-class-unbounded-ltr",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/deletion-reinserted-feature-unapply",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/feature-rule-before-merge",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/infix-interdigitation",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/mpr-gated-exception",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/simultaneous-feeding",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/simultaneous-feeding-control-iterative",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/two-merges-cascade-env",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:languages/polysynthetic-stratal-derivation-chain",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:languages/templatic-root-modification",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/backend-gated-generic",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/backend-strata-generic",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/cascade-symbolic-output",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/loader-default-symbol",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/narrowing-boundary-first-alpha-lhs",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/narrowing-boundary-order-alpha-lhs",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/rewrite-default-narrow",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/unattested-intermediate-bundle",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/unbound-pair-expansion",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/unspecified-feature-minus-target",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/alpha-variable-name-collision",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/ambiguous-disagree-mixed-plus-class",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/nullable-disagree-bounded-ltr-left",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/nullable-disagree-bounded-ltr-right",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/nullable-disagree-bounded-rtl-left",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/nullable-disagree-bounded-rtl-right",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/nullable-disagree-plain-ltr-right",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/nullable-disagree-unbounded-ltr-left",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/nullable-disagree-unbounded-ltr-right",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/nullable-disagree-unbounded-rtl-left",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/nullable-disagree-unbounded-rtl-right",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/partial-class-disagree-bounded-ltr-left",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/partial-class-disagree-bounded-rtl-left",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/partial-class-disagree-bounded-rtl-right",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/partial-class-disagree-unbounded-ltr-right",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/partial-class-disagree-unbounded-rtl-left",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/partial-class-disagree-unbounded-rtl-right",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/feature-system-breadth",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/intermediate-alpha-target",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/permissive-intermediate-class",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "staging:edge-cases/unspecified-feature-minus-environment",
        ProposalRecallGap::AmbiguousSegmentSpelling,
    ),
    (
        "machine:edge-cases/circumfix-cross-product-and-infix-drop",
        ProposalRecallGap::TruncationComposition,
    ),
    (
        "machine:edge-cases/deletion-reduplication-exception-composite",
        ProposalRecallGap::PhonologicalReduplication,
    ),
    (
        "machine:edge-cases/morphotactic-attribute-breadth",
        ProposalRecallGap::TemplateReentry,
    ),
    (
        "machine:edge-cases/truncate-morphotactic",
        ProposalRecallGap::TruncationComposition,
    ),
    (
        "machine:edge-cases/truncate-morphotactic-identity",
        ProposalRecallGap::TruncationComposition,
    ),
    (
        "machine:edge-cases/truncate-morphotactic-subsumption",
        ProposalRecallGap::TruncationComposition,
    ),
    (
        "machine:languages/suffixing-extension-slot-ordering",
        ProposalRecallGap::PhonologicalReduplication,
    ),
    (
        "staging:edge-cases/unordered-independent-affix-sites",
        ProposalRecallGap::TruncationComposition,
    ),
    (
        "staging:languages/suffixing-extension-slot-ordering-single-stem-name",
        ProposalRecallGap::PhonologicalReduplication,
    ),
];

#[test]
fn known_recall_gaps_refuse_before_construction_with_typed_causes() {
    let fixtures = discover();
    for &(label, expected) in RECALL_REGRESSIONS.iter().chain(COLLATERAL_REFUSALS) {
        let fixture = fixtures.iter().find(|f| f.label() == label).expect(label);
        let grammar = fixture.load_grammar();
        let (result, profile) = pg_foma::analyzer::compile_proposer_with_profile(&grammar);
        let Err(pg_foma::analyzer::FomaError::CapabilityRefused(diagnostics)) = result else {
            panic!("{label}: expected typed capability refusal");
        };
        assert!(
            diagnostics
                .iter()
                .any(|d| d.refusal == CapabilityRefusal::ProposalRecall(expected)),
            "{label}: expected {expected:?}, got {diagnostics:?}"
        );
        assert!(
            profile.final_state_count.is_none(),
            "{label}: built an artifact before refusal"
        );
        let emitted = pg_foma::emit::emit_tuned_surface(&grammar);
        assert!(
            matches!(
                emitted.report.tier,
                pg_foma::emit::FomaTier::CannotRepresent { .. }
            ),
            "{label}: the emitter must enforce the same representability limit"
        );
        assert!(
            emitted.lexc_source.is_empty(),
            "{label}: emitted partial source"
        );
        let selection = pg_foma::backend_selection::select_backends_for_grammar(&grammar);
        let report = selection
            .report_for(EmissionStrategy::TunedSurfaceProbed)
            .expect(label);
        assert!(
            !report.can_represent(),
            "{label}: selector admitted a recall gap"
        );
        assert!(
            report.findings().iter().any(|finding| {
                finding.class() == pg_health::health::FindingClass::Representability
                    && finding.severity == pg_health::health::Severity::CannotRepresent
            }),
            "{label}: refusal must be representability, not readiness or containment"
        );
    }
}

#[test]
fn recall_refusals_follow_referenced_constructs_even_when_names_are_unchanged() {
    use pg_foma::capability::{CompileDecision, GrammarWideCheck, ProposalRecallCheck};
    use pg_foma::grammar_semantics::GrammarSemantics;
    use pg_foma::plan::Plan;

    let fixtures = discover();
    for &(label, gap) in RECALL_REGRESSIONS {
        let fixture = fixtures.iter().find(|f| f.label() == label).expect(label);
        let mut grammar = fixture.load_grammar();
        match gap {
            ProposalRecallGap::AmbiguousSegmentSpelling => {
                for stratum in &mut grammar.strata {
                    stratum.prules.clear();
                    stratum.entries.clear();
                }
            }
            ProposalRecallGap::TemplateReentry => {
                for template in &mut grammar.templates {
                    template.is_final = true;
                }
            }
            ProposalRecallGap::TruncationComposition => {
                for stratum in &mut grammar.strata {
                    stratum.mrules.truncate(1);
                }
            }
            ProposalRecallGap::PartialRuleDeletionOrReduplication => {
                for rule in &mut grammar.mrules {
                    if let pg_grammar::model::MorphRuleDef::AffixProcess(def) = rule {
                        def.partial_reason = None;
                    }
                }
            }
            _ => {
                for stratum in &mut grammar.strata {
                    stratum.prules.clear();
                }
            }
        }
        let semantics = GrammarSemantics::derive(&grammar);
        let phonology = pg_foma::junctions::PhonologyProbe::new_with_semantics(&semantics);
        let plan: Plan = pg_foma::enumerate::enumerate_default(
            &grammar,
            semantics.prules_in_order(),
            phonology.as_ref(),
        );
        if let Some(CompileDecision::Refuse(diagnostics)) =
            ProposalRecallCheck.evaluate(&semantics, &plan)
        {
            assert!(
                !diagnostics
                    .iter()
                    .any(|d| d.refusal == CapabilityRefusal::ProposalRecall(gap)),
                "{label}: removing the construct must remove {gap:?}, got {diagnostics:?}"
            );
        }
    }
}

#[test]
fn literal_rewrite_and_unreferenced_classes_retain_complete_recall() {
    let fixtures = discover();
    for (label, remove_rules) in [
        (
            "staging:edge-cases/featureless-single-member-class-rewrite",
            false,
        ),
        ("machine:edge-cases/featureless-literal-target", true),
    ] {
        let fixture = fixtures.iter().find(|f| f.label() == label).expect(label);
        let mut xml = fixture.load_grammar_xml();
        if !remove_rules {
            for (class, segment) in [("ncK", "cK"), ("ncS", "cS")] {
                let context = format!(r#"<SimpleContext naturalClass="{class}" />"#);
                assert_eq!(xml.matches(&context).count(), 1);
                xml = xml.replace(&context, &format!(r#"<Segment segment="{segment}" />"#));
            }
        }
        let mut grammar = pg_grammar::load(&xml).expect("literal rewrite control must load");
        if remove_rules {
            for stratum in &mut grammar.strata {
                stratum.prules.clear();
            }
        }
        let words: Vec<_> = fixture
            .load_words_yaml()
            .words
            .into_iter()
            .map(|w| w.word)
            .collect();
        let row = pg_foma_backend::scoreboard::measure(label, &grammar, &words);
        let cell = row
            .cells
            .iter()
            .find(|c| c.strategy == EmissionStrategy::TunedSurfaceProbed)
            .expect(label);
        assert!(
            matches!(
                cell.outcome,
                pg_foma_backend::scoreboard::CellOutcome::OracleExact
            ),
            "{label}: {:?}",
            cell.outcome
        );
    }
}

// Length-preserving rewrites of three-segment roots make alphabet^3 an exhaustive surface corpus.
fn inventory_xml(
    features: usize,
    alias: bool,
    voices: [usize; 3],
    lhs: usize,
    rhs: usize,
) -> String {
    let feature_system = if features == 0 {
        String::new()
    } else {
        format!(
            r#"<PhonologicalFeatureSystem>
            <SymbolicFeature id="cat"><Name>category</Name><Symbols>
              <Symbol id="A">A</Symbol><Symbol id="K">K</Symbol>
              <Symbol id="S">S</Symbol><Symbol id="Z">Z</Symbol>
            </Symbols></SymbolicFeature>{}</PhonologicalFeatureSystem>"#,
            if features == 2 {
                r#"<SymbolicFeature id="voice"><Name>voice</Name><Symbols>
                <Symbol id="minus">minus</Symbol><Symbol id="plus">plus</Symbol>
                </Symbols></SymbolicFeature>"#
            } else {
                ""
            }
        )
    };
    let mut segments = String::new();
    for (index, (id, spelling, category)) in [
        ("cA", "a", "A"),
        ("cK", "k", "K"),
        ("cS", "s", "S"),
        ("cZ", "z", "Z"),
    ]
    .into_iter()
    .enumerate()
    {
        if alias && id == "cZ" {
            continue;
        }
        let category = if features == 2 && id == "cZ" {
            "S"
        } else {
            category
        };
        let voice = if index == 0 { 1 } else { voices[index - 1] };
        segments.push_str(&format!(
            r#"<SegmentDefinition id="{id}"><Representations><Representation>{spelling}</Representation>{}</Representations>{}{}</SegmentDefinition>"#,
            if alias && id == "cS" { "<Representation>z</Representation>" } else { "" },
            if features > 0 { format!(r#"<FeatureValue feature="cat" symbolValues="{category}"/>"#) } else { String::new() },
            if features == 2 && voice > 0 {
                format!(r#"<FeatureValue feature="voice" symbolValues="{}"/>"#, if voice == 1 { "minus" } else { "plus" })
            } else { String::new() }
        ));
    }
    let context = |side: &str, syntax| match syntax {
        0 => format!(r#"<Segment segment="c{side}"/>"#),
        1 => format!(r#"<SimpleContext naturalClass="seg{side}"/>"#),
        2 => format!(r#"<SimpleContext naturalClass="feat{side}"/>"#),
        _ => unreachable!(),
    };
    let classes: String = ["K", "S"].into_iter().map(|side| format!(
        r#"<SegmentNaturalClass id="seg{side}"><Name>segment</Name><Segment segment="c{side}"/></SegmentNaturalClass>
        <FeatureNaturalClass id="feat{side}"><Name>feature</Name>{}</FeatureNaturalClass>"#,
        if features > 0 { format!(r#"<FeatureValue feature="cat" symbolValues="{side}"/>"#) } else { String::new() }
    )).collect();
    format!(
        r#"<HermitCrabInput><Language><Name>GeneratedSpellingInventory</Name>
        <PartsOfSpeech><PartOfSpeech id="pos"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
        {feature_system}<CharacterDefinitionTable id="tbl"><Name>Main</Name><SegmentDefinitions>{segments}</SegmentDefinitions></CharacterDefinitionTable>
        <NaturalClasses>{classes}</NaturalClasses><PhonologicalRuleDefinitions>
        <PhonologicalRule id="rewrite"><Name>rewrite</Name><PhoneticInput><PhoneticSequence>{}</PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence>{}</PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
        </PhonologicalRule></PhonologicalRuleDefinitions><Strata><Stratum characterDefinitionTable="tbl" phonologicalRules="rewrite" morphologicalRuleOrder="unordered"><Name>Main</Name>
        <LexicalEntries><LexicalEntry id="root" partOfSpeech="pos"><Allomorphs><Allomorph id="rootAllomorph"><PhoneticShape>aka</PhoneticShape></Allomorph></Allomorphs><MorphemeId>ROOT</MorphemeId><Gloss>root</Gloss></LexicalEntry></LexicalEntries>
        </Stratum></Strata></Language></HermitCrabInput>"#,
        context("K", lhs),
        context("S", rhs)
    )
}

#[test]
fn generated_inventory_admission_preserves_every_hc_proposal() {
    use pg_foma_backend::scoreboard::{measure, CellOutcome};
    let mut words = Vec::new();
    for a in ['a', 'k', 's', 'z'] {
        for b in ['a', 'k', 's', 'z'] {
            for c in ['a', 'k', 's', 'z'] {
                words.push(format!("{a}{b}{c}"));
            }
        }
    }
    let mut admitted = 0;
    let mut refused = 0;
    let mut failures = Vec::new();
    for features in 0..=2 {
        let assignments = if features == 2 { 27 } else { 1 };
        for assignment in 0..assignments {
            let voices = [assignment % 3, assignment / 3 % 3, assignment / 9 % 3];
            for alias in [false, true] {
                for lhs in 0..3 {
                    for rhs in 0..3 {
                        let label = format!("features={features},alias={alias},voices={voices:?},lhs={lhs},rhs={rhs}");
                        let grammar =
                            pg_grammar::load(&inventory_xml(features, alias, voices, lhs, rhs))
                                .expect(&label);
                        let row = measure(&label, &grammar, &words);
                        assert!(
                            row.excluded_words.is_empty(),
                            "{label}: {:?}",
                            row.excluded_words
                        );
                        let cell = row
                            .cells
                            .iter()
                            .find(|c| c.strategy == EmissionStrategy::TunedSurfaceProbed)
                            .unwrap();
                        match &cell.outcome {
                            CellOutcome::Refused { .. } => {
                                let Err(pg_foma::analyzer::FomaError::CapabilityRefused(_)) =
                                    pg_foma::analyzer::compile_proposer(&grammar)
                                else {
                                    panic!("{label}: an operational failure is not a capability refusal");
                                };
                                refused += 1;
                            }
                            CellOutcome::OracleExact => {
                                admitted += 1;
                                assert_eq!(
                                    cell.words_measured,
                                    Some(words.len()),
                                    "{label}: incomplete comparison"
                                );
                                let divergence = cell.divergence.expect("comparable identities");
                                assert!(
                                    divergence.oracle_identities > 0,
                                    "{label}: no positive HC control"
                                );
                                assert_eq!(divergence.oracle_only_identities, 0, "{label}");
                                assert_eq!(divergence.candidate_only_identities, 0, "{label}");
                            }
                            other => failures.push(format!("{label}: {other:?}")),
                        }
                    }
                }
            }
        }
    }
    for failure in &failures {
        eprintln!("GENERATED_SPELLING_FAILURE\t{failure}");
    }
    eprintln!(
        "GENERATED_SPELLING_CENSUS admitted={admitted} refused={refused} failures={}",
        failures.len()
    );
    assert!(
        admitted > 0 && refused > 0,
        "both sides of admission must execute"
    );
    assert!(
        failures.is_empty(),
        "admitted inventories lose HC identities: {failures:#?}"
    );
    assert_eq!(admitted + refused, 522);
}

fn refuses_partial_rule_gap(grammar: &pg_grammar::model::Grammar) -> bool {
    use pg_foma::analyzer::{compile_proposer, FomaError};
    match compile_proposer(grammar) {
        Err(FomaError::CapabilityRefused(refusals)) => refusals.iter().any(|d| {
            d.refusal
                == CapabilityRefusal::ProposalRecall(
                    ProposalRecallGap::PartialRuleDeletionOrReduplication,
                )
        }),
        _ => false,
    }
}

// Probes a narrowing rewrite `o t -> t` and a reduplication in a stratum before the partial rule.
const NARROWING_PROBE: &str = include_str!("fixtures/partial-rule-gap-probes/p2-narrowing.xml");
const EARLY_REDUP_PROBE: &str = include_str!("fixtures/partial-rule-gap-probes/p1-early-redup.xml");
// A partial rule that only a template slot names.
const SLOT_PARTIAL_PROBE: &str =
    include_str!("fixtures/partial-rule-gap-probes/p3-slot-partial.xml");

#[test]
fn a_partial_rule_with_a_deletion_or_reduplication_is_refused() {
    let fixtures = discover();
    let load = |name: &str| {
        let label = format!("machine:edge-cases/{name}");
        fixtures
            .iter()
            .find(|f| f.label() == label)
            .expect("fixture is pinned")
            .load_grammar()
    };
    let clear_partial = |grammar: &mut pg_grammar::model::Grammar| {
        for rule in &mut grammar.mrules {
            if let pg_grammar::model::MorphRuleDef::AffixProcess(def) = rule {
                def.partial_reason = None;
            }
        }
    };
    let mut cases: Vec<(&str, pg_grammar::model::Grammar)> = vec![
        (
            "partial-rule-deleted-material",
            load("partial-rule-deleted-material"),
        ),
        (
            "partial-rule-wrapped-material",
            load("partial-rule-wrapped-material"),
        ),
        (
            "p2-narrowing",
            pg_grammar::load(NARROWING_PROBE).expect("probe loads"),
        ),
        (
            "p1-early-redup",
            pg_grammar::load(EARLY_REDUP_PROBE).expect("probe loads"),
        ),
        (
            "p3-slot-partial",
            pg_grammar::load(SLOT_PARTIAL_PROBE).expect("probe loads"),
        ),
    ];
    for (name, grammar) in &mut cases {
        assert!(refuses_partial_rule_gap(grammar), "{name}");
        clear_partial(grammar);
        assert!(
            !refuses_partial_rule_gap(grammar),
            "{name}: without a partial rule the refusal must not apply"
        );
    }
    assert!(
        !refuses_partial_rule_gap(&load("partial-rule-infixed-material")),
        "a partial rule with neither a deletion nor a reduplication stays admitted"
    );
}

fn refuses_gap(grammar: &pg_grammar::model::Grammar, gap: ProposalRecallGap) -> bool {
    use pg_foma::analyzer::{compile_proposer, FomaError};
    match compile_proposer(grammar) {
        Err(FomaError::CapabilityRefused(refusals)) => refusals
            .iter()
            .any(|d| d.refusal == CapabilityRefusal::ProposalRecall(gap)),
        _ => false,
    }
}

// Minimal repros of #415 (`lipt`, `ktip`) and #416 (`loplopt`, `loplopkor`): each loses an analysis today.
const JUNCTION_LIPT: &str = include_str!("fixtures/partial-rule-gap-probes/j1-lipt.xml");
const JUNCTION_KTIP: &str = include_str!("fixtures/partial-rule-gap-probes/j2-ktip.xml");
const JUNCTION_Q6: &str = include_str!("fixtures/partial-rule-gap-probes/q6-min-ctxfree.xml");
const JUNCTION_Q1: &str = include_str!("fixtures/partial-rule-gap-probes/q1-narrow-noenv.xml");
const JUNCTION_Q3: &str = include_str!("fixtures/partial-rule-gap-probes/q3-feed.xml");
const JUNCTION_J2NOENV: &str = include_str!("fixtures/partial-rule-gap-probes/j2-ktip-noenv.xml");
const JUNCTION_T1: &str = include_str!("fixtures/partial-rule-gap-probes/t1-cf-narrow.xml");
const JUNCTION_T2: &str =
    include_str!("fixtures/partial-rule-gap-probes/t2-change-feeds-cfdel.xml");
const JUNCTION_T4: &str =
    include_str!("fixtures/partial-rule-gap-probes/t4-bdel-feeds-cfnarrow.xml");
const REDUP_WRAP_NP_NT: &str = include_str!("fixtures/partial-rule-gap-probes/r3-wrap-np-nt.xml");
const REDUP_U9: &str = include_str!("fixtures/partial-rule-gap-probes/u9-redup-compound.xml");
const REDUP_U11: &str =
    include_str!("fixtures/partial-rule-gap-probes/u11-redup-compound-same.xml");
const REDUP_LOPLOPT: &str = include_str!("fixtures/partial-rule-gap-probes/r1-loplopt.xml");
const REDUP_LOPLOPKOR: &str = include_str!("fixtures/partial-rule-gap-probes/r2-loplopkor.xml");

#[test]
fn a_deletion_across_a_junction_is_refused() {
    for (name, xml) in [
        ("lipt", JUNCTION_LIPT),
        ("ktip", JUNCTION_KTIP),
        ("slot-partial", SLOT_PARTIAL_PROBE),
        ("q6-min-ctxfree", JUNCTION_Q6),
        ("q1-narrow-noenv", JUNCTION_Q1),
        ("q3-feed", JUNCTION_Q3),
        ("j2-ktip-noenv", JUNCTION_J2NOENV),
        ("t1-cf-narrow", JUNCTION_T1),
        ("t2-change-feeds-cfdel", JUNCTION_T2),
        ("t4-bdel-feeds-cfnarrow", JUNCTION_T4),
    ] {
        let grammar = pg_grammar::load(xml).expect("probe loads");
        assert!(
            refuses_gap(&grammar, ProposalRecallGap::JunctionDeletion),
            "{name}"
        );
    }
}

#[test]
fn a_reduplication_before_a_later_affix_is_refused() {
    for (name, xml) in [
        ("loplopt", REDUP_LOPLOPT),
        ("loplopkor", REDUP_LOPLOPKOR),
        ("wrap-np-nt", REDUP_WRAP_NP_NT),
        ("u9-redup-compound", REDUP_U9),
        ("u11-redup-compound-same", REDUP_U11),
    ] {
        let grammar = pg_grammar::load(xml).expect("probe loads");
        assert!(
            refuses_gap(&grammar, ProposalRecallGap::ReduplicationBeforeLaterAffix),
            "{name}"
        );
    }
}
