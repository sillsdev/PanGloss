use super::*;

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
