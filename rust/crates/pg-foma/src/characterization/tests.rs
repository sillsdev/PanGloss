use super::*;
use crate::backend_selection::best_case_across_backends;

/// A clean grammar (no Refuse/ConfirmOnly construct, no unbounded quantifier, small rule-interaction product) must raise no characterization finding at all.
#[test]
fn characterization_raises_nothing_for_a_clean_small_grammar() {
    const CLEAN_XML: &str = r#"<HermitCrabInput><Language><Name>CharacterizationCleanFixture</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>main</Name>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="e1-1"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>kat</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let grammar =
        pg_grammar::load(CLEAN_XML).unwrap_or_else(|e| panic!("fixture load failed: {e}"));
    assert_eq!(
        best_case_across_backends(&GrammarSemantics::derive(&grammar)),
        CompileDecision::Admit
    );
    let findings = characterization_findings(&grammar);
    assert!(
        findings.is_empty(),
        "a clean, tiny grammar must raise no characterization finding: {findings:?}"
    );
}

/// A `Refuse` verdict must produce a `CannotRepresent` finding naming the construct; the fixture reduplicates on a `RealizationalRule` because only a construct EVERY compiler declines reaches the JOIN.
/// See docs/research/pg-foma-capability-design-notes.md.
#[test]
fn characterization_raises_cannot_represent_finding_for_refuse_verdict() {
    const REFUSE_XML: &str = r#"<HermitCrabInput><Language><Name>RedupRealizational</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="rrRedupBad">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <RealizationalRule id="rrRedupBad">
                  <Name>redupBad</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="subRedupBad">
                      <MorphologicalInput>
                        <PhoneticSequence id="qA"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </MorphologicalInput>
                      <MorphologicalOutput redupMorphType="suffix">
                        <CopyFromInput index="qA" />
                        <CopyFromInput index="qA" />
                      </MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                  <MorphemeId>REDBAD</MorphemeId>
                </RealizationalRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let grammar =
        pg_grammar::load(REFUSE_XML).unwrap_or_else(|e| panic!("fixture load failed: {e}"));
    assert!(matches!(
        best_case_across_backends(&GrammarSemantics::derive(&grammar)),
        CompileDecision::Refuse(_)
    ));
    let findings = characterization_findings(&grammar);

    let finding = findings
        .iter()
        .find(|f| f.severity == Severity::CannotRepresent)
        .unwrap_or_else(|| {
            panic!("expected a CannotRepresent characterization finding, got {findings:?}")
        });
    assert_eq!(finding.code, FindingCode::BackendCoverageIncomplete);
    assert_eq!(finding.metric, Metric::BackendCoverageGapCount);
    assert_eq!(
        finding.value,
        MetricValue::Count(finding.affected.len() as u64)
    );
    assert_eq!(finding.phase, Phase::Characterization);
    assert_eq!(finding.provenance, ValueProvenance::Observed);
    assert!(
        finding
            .affected
            .iter()
            .any(|a| a.contains("mrule 0 allomorph #0")),
        "expected the non-peel-eligible reduplication construct named: {finding:?}"
    );
}

/// Crossing `RULE_PRODUCT_WARNING_THRESHOLD` raises a `Predicted`/`LargeMultiplier` finding naming the exact product; exercised directly against a synthetic `CharacteristicsProfile` since this finding depends on nothing else in the profile.
#[test]
fn rule_interaction_product_finding_fires_above_threshold_not_below() {
    let mut above = CharacteristicsProfile::default();
    above.cardinality.mrule_count = 9;
    above.cardinality.prule_count = 9; // 81 > 64
    let finding = rule_interaction_product_finding(&above)
        .unwrap_or_else(|| panic!("expected a rule-interaction-product finding"));
    assert_eq!(finding.code, FindingCode::RuleInteractionProduct);
    assert_eq!(finding.severity, Severity::LargeMultiplier);
    assert_eq!(finding.provenance, ValueProvenance::Predicted);
    assert_eq!(finding.phase, Phase::Characterization);
    assert_eq!(finding.value, MetricValue::Count(81));
    assert!(finding.affected.is_empty());

    let mut below = CharacteristicsProfile::default();
    below.cardinality.mrule_count = 2;
    below.cardinality.prule_count = 2; // 4 <= 64
    assert!(rule_interaction_product_finding(&below).is_none());
}

/// A grammar one compiler accepts and another refuses is not refused as a whole, yet used to produce no finding at all (G15); the finding now names the declining compilers.
#[test]
fn a_partly_refused_envelope_yields_a_finding_naming_the_declining_compilers() {
    use crate::capability::{CapabilityDiagnostic, StrategyVerdict};
    use crate::strategy_coverage::ALL_STRATEGIES;
    let diagnostic = CapabilityDiagnostic {
        predicate: "some-predicate",
        construct: "mrule 0".to_string(),
        witness: "w".to_string(),
    };
    let decisions = |declining: &[usize]| {
        StrategyEnvelope::from_verdicts(
            ALL_STRATEGIES
                .iter()
                .copied()
                .enumerate()
                .map(|(i, strategy)| StrategyVerdict {
                    strategy,
                    decision: if declining.contains(&i) {
                        CompileDecision::Refuse(vec![diagnostic.clone()])
                    } else {
                        CompileDecision::Admit
                    },
                })
                .collect(),
        )
    };

    let partial = decisions(&[0]);
    assert_eq!(partial.global(), CompileDecision::Admit);
    let finding = partial_refusal_finding(&partial).expect("a declining compiler must be reported");
    assert_eq!(finding.code, FindingCode::BackendCoverageIncomplete);
    assert_eq!(finding.value, MetricValue::Count(1));
    assert_eq!(finding.affected, vec!["mrule 0".to_string()]);
    assert!(
        finding
            .explanation
            .contains(&format!("{:?} declines", ALL_STRATEGIES[0])),
        "the finding must name the declining compiler: {finding:?}"
    );

    assert!(
        partial_refusal_finding(&decisions(&[])).is_none(),
        "nobody declines"
    );
    let everyone: Vec<usize> = (0..ALL_STRATEGIES.len()).collect();
    assert!(
        partial_refusal_finding(&decisions(&everyone)).is_none(),
        "a whole-grammar refusal is the other finding's job"
    );
}

/// Production wiring on a real grammar: the templated compiler declines this circumfix grammar while another compiler admits it, so the whole grammar is not refused and the walker must still say which compiler declined. Census over all 134 conformance fixtures: 21 are partly refused, 1 wholly refused, and every partly refused one names a real declining compiler.
#[test]
fn a_real_partly_refused_grammar_is_reported_through_characterization_findings() {
    let fixture = pg_conformance_fixtures::require_fixture(
        "edge-cases",
        "circumfix-infix-interior-action-precedence",
    );
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture grammar loads");
    assert!(!matches!(
        best_case_across_backends(&GrammarSemantics::derive(&grammar)),
        CompileDecision::Refuse(_)
    ));
    let findings = characterization_findings(&grammar);
    let finding = findings
        .iter()
        .find(|f| f.code == FindingCode::BackendCoverageIncomplete)
        .unwrap_or_else(|| panic!("expected the partial-refusal finding, got {findings:?}"));
    assert_eq!(finding.severity, Severity::Elevated);
    assert!(
        finding
            .explanation
            .contains("TemplatedUnderlyingTokens declines"),
        "{finding:?}"
    );
}
