use super::*;

/// Every registered predicate's and grammar-wide check's own declared shape key must be a real catalog entry, or `refused`'s `expect("every capability-refusal shape must exist in the advice catalog")` is one bad key away from panicking in production.
#[test]
fn every_declared_shape_key_exists_in_the_advice_catalog() {
    let catalog = builtin_catalog().expect("the embedded backend advice catalog must validate");
    for predicate in default_registry().predicates() {
        let key = predicate.shape_key();
        assert!(
            catalog.entry_for(key).is_some(),
            "{:?} declares shape key {key:?}, which has no catalog entry",
            predicate.id()
        );
    }
    for check in default_grammar_wide_checks() {
        let key = check.shape_key();
        assert!(
            catalog.entry_for(key).is_some(),
            "{:?} declares shape key {key:?}, which has no catalog entry",
            check.id()
        );
    }
}

const ADMIT_XML: &str = r#"<HermitCrabInput><Language><Name>BackendSelectionFixture</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <Strata>
        <Stratum characterDefinitionTable="t1">
          <Name>S</Name>
          <LexicalEntries>
            <LexicalEntry id="e1"><Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>
          </LexicalEntries>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

/// Mirrors `tests/admission_single_owner_gate.rs`'s whole-fixture-set measurement as a unit test.
#[test]
fn every_all_strategies_member_is_reported() {
    let g = pg_grammar::load(ADMIT_XML).expect("fixture must load");
    let selection = select_backends_for_grammar(&g);
    for &strategy in ALL_STRATEGIES {
        assert!(
            selection.report_for(strategy).is_some(),
            "{strategy:?} has no report; decision_for's fail-closed arm just became live policy"
        );
    }
}

/// A composed report's own decision passes through unchanged.
#[test]
fn decision_for_returns_the_composed_reports_own_decision() {
    let g = pg_grammar::load(ADMIT_XML).expect("fixture must load");
    let selection = select_backends_for_grammar(&g);
    for &strategy in ALL_STRATEGIES {
        let expected = selection
            .report_for(strategy)
            .expect("pinned above: every strategy has a report")
            .decision()
            .clone();
        assert_eq!(selection.decision_for(strategy), expected);
    }
}

// Folded in from the former `capability_entry.rs`, same fixtures and expected verdicts.

/// An ordinary affix + iterative-rewrite grammar must evaluate to `Admit` through `best_case` too.
#[test]
fn best_case_admits_ordinary_affix_and_iterative_rewrite_grammar() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>Ordinary</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cb" /></SegmentNaturalClass></NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="pr1" morphologicalRules="mr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <MorphologicalRule id="mr1">
                  <Name>-a</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput>
                        <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence>
                      </MorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="stem" />
                        <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                      </MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </MorphologicalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = pg_grammar::load(XML).expect("fixture must load");

    assert_eq!(
        best_case_across_backends(&GrammarSemantics::derive(&g)),
        CompileDecision::Admit
    );
}

/// A single, non-recursive `Compounding` rule must evaluate to `ConfirmOnly` through `best_case` too.
#[test]
fn best_case_confirm_only_for_non_recursive_compounding_grammar() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = pg_grammar::load(XML).expect("fixture must load");

    assert_eq!(
        best_case_across_backends(&GrammarSemantics::derive(&g)),
        CompileDecision::ConfirmOnly
    );
}

/// A self-feeding (`multipleApplication="2"`) `Compounding` rule evaluates to `ConfirmOnly` through `best_case` too, not bare `Refuse`.
#[test]
fn best_case_confirm_only_for_recursive_compounding_grammar() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1" multipleApplication="2">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
    let g = pg_grammar::load(XML).expect("fixture must load");

    assert_eq!(
        best_case_across_backends(&GrammarSemantics::derive(&g)),
        CompileDecision::ConfirmOnly
    );
}

#[test]
fn alpha_environment_refuses_all_real_compilers_and_the_selector() {
    let xml = include_str!("../../../pg-parse/tests/data/alpha-optional-environment.xml");
    let grammar = pg_grammar::load(xml).expect("alpha environment fixture loads");
    assert!(crate::emit::alpha_environment_refusal(&grammar));
    let selection = select_backends_for_grammar(&grammar);
    for &strategy in ALL_STRATEGIES {
        assert!(
            matches!(selection.decision_for(strategy), CompileDecision::Refuse(_)),
            "{strategy:?}"
        );
    }
    let tuned = crate::emit::emit_tuned_surface(&grammar);
    assert!(matches!(
        tuned.report.tier,
        crate::emit::FomaTier::CannotRepresent { .. }
    ));
    assert!(tuned.lexc_source.is_empty());
    let alphabet = crate::replace::SegAlphabet::new(&grammar.char_tables[0]);
    let templated = crate::emit::emit_underlying_templated(&grammar, &alphabet, None);
    assert!(matches!(
        templated.report.tier,
        crate::emit::FomaTier::CannotRepresent { .. }
    ));
    assert!(templated.lexc_source.is_empty());
    assert!(matches!(
        crate::analyzer::compile_proposer_unproven(&grammar),
        Err(crate::analyzer::FomaError::Unsupported(_))
    ));
    let direct = crate::gate::compile_gated_grammar(
        &foma::options::FomaOptions::default(),
        &grammar,
        &alphabet,
        &[],
    );
    assert!(matches!(
        direct,
        Err(crate::compose_budget::ComposeError::CannotRepresent { .. })
    ));
    let result = crate::build::build_controllable(
        &crate::plan::Plan::new(),
        &foma::options::FomaOptions::default(),
        &grammar,
        &alphabet,
        &[],
    );
    assert!(matches!(
        result,
        Err(crate::compose_budget::ComposeError::CannotRepresent { .. })
    ));
    let plain = pg_grammar::load(ADMIT_XML).expect("plain fixture loads");
    assert!(!crate::emit::alpha_environment_refusal(&plain));
    assert!(!matches!(
        crate::emit::emit_tuned_surface(&plain).report.tier,
        crate::emit::FomaTier::CannotRepresent { .. }
    ));
}
