use super::*;
use pg_shape::ShapeBuilder;

#[test]
fn grammar_health_report_uses_v2_envelope() {
    let report = GrammarHealthReport::new(Vec::new()).expect("empty report is valid");
    let json = serde_json::to_value(report).expect("report serializes");

    assert_eq!(json["schema_version"], 2);
    assert!(json["fieldworks_project"].is_object());
    assert!(json["summary"].is_array());
    assert!(json["findings"].is_array());
}

#[test]
fn blank_named_import_subject_still_assembles_into_a_report() {
    let warning = pg_snapshot::Warning::new("grammar.template.no-slots", "Empty template.")
        .with_subject(
            pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::MoInflAffixTemplate)
                .guid("12345678-1234-1234-1234-123456789abc")
                .name(" "),
        );
    let finding = GrammarHealthCheckFinding::from_import_warning(&warning);
    assert_eq!(finding.subjects[0].title, "Unnamed affix template");
    GrammarHealthReport::new(vec![finding])
        .expect("a blank FieldWorks name must not refuse the report");
}

#[test]
fn import_warning_constructor_preserves_source_identity_without_compiled_ids() {
    let code = ImportWarningCode::NaturalClassUnreferencedCompacted.wire();
    let warning = pg_snapshot::Warning::new(code, "Unused natural class.").with_subject(
        pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::PhNaturalClass)
            .guid("12345678-1234-1234-1234-123456789abc")
            .name("unnamed natural class"),
    );

    let finding = GrammarHealthCheckFinding::from_import_warning(&warning);

    assert_eq!(finding.code.wire(), code);
    assert_eq!(finding.origin, FindingOrigin::Import);
    assert_eq!(finding.audience, Audience::Developer);
    assert_eq!(finding.message, "Unused natural class.");
    assert_eq!(finding.guidance, None);
    assert_eq!(finding.subjects[0].internal_id, None);
    assert_eq!(finding.subjects[0].title, "unnamed natural class");
    assert_eq!(
        finding.subjects[0].kind,
        pg_snapshot::FwClass::PhNaturalClass
    );
    assert_eq!(
        finding.subjects[0].guid.as_deref(),
        Some("12345678-1234-1234-1234-123456789abc")
    );
}

#[test]
fn developer_only_import_warning_keeps_developer_audience() {
    let warning = pg_snapshot::Warning::new(
        ImportWarningCode::NaturalClassUnreferencedCompacted.wire(),
        "natural class 7 unreferenced after compaction",
    )
    .with_subject(
        pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::PhNaturalClass)
            .guid("12345678-1234-1234-1234-123456789abc")
            .name("unnamed natural class"),
    );

    let finding = GrammarHealthCheckFinding::from_import_warning(&warning);

    assert_eq!(finding.audience, Audience::Developer);
    assert_eq!(finding.subjects[0].title, "unnamed natural class");
}

#[test]
fn import_warning_uses_human_group_name_and_table_guidance() {
    let warning = pg_snapshot::Warning::new(
        "grammar.template.no-slots",
        "affix template has no slots with any loaded affix rule",
    );

    let finding = GrammarHealthCheckFinding::from_import_warning(&warning);

    assert_eq!(finding.group_name, "Empty affix template");
    let guidance = finding.guidance.expect("import metadata supplies guidance");
    assert!(guidance.contains(pg_snapshot::fieldworks_paths::GRAMMAR_CATEGORY_AFFIX_TEMPLATES));
}

#[test]
fn import_warning_guidance_template_uses_its_subject_name() {
    let warning = pg_snapshot::Warning::new(
        ImportWarningCode::EnvironmentInvalid.wire(),
        "Phonological environment 'bad environment' is invalid.",
    )
    .with_subject(
        pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::PhEnvironment).name("bad environment"),
    );

    let finding = GrammarHealthCheckFinding::from_import_warning(&warning);

    assert_eq!(
        finding.guidance.as_deref(),
        Some("In Grammar > Environments, correct the expression for phonological environment 'bad environment'.")
    );
}

fn grammar(xml: &str) -> Grammar {
    crate::load(xml).unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
}

fn codes(findings: &[GrammarHealthCheckFinding]) -> Vec<GrammarHealthCode> {
    findings.iter().map(|f| f.code.clone()).collect()
}

#[test]
fn every_code_has_a_distinct_stable_linguist_group_name() {
    let mut names = Vec::new();
    for code in GrammarHealthCode::ALL {
        let name = code.group_name();
        assert!(!name.trim().is_empty(), "{code:?} has no group name");
        assert!(
            name.chars().count() <= 30,
            "{code:?} group name is too long: {name}"
        );
        assert!(!names.contains(&name), "duplicate group name: {name}");
        names.push(name);
    }
}

#[test]
fn every_import_warning_code_has_exactly_one_metadata_entry() {
    let mut wires = Vec::new();
    for code in ImportWarningCode::ALL {
        let metadata = pg_snapshot::import_warning_metadata(*code);
        assert!(!metadata.group_name.trim().is_empty(), "{code:?}");
        assert_eq!(ImportWarningCode::from_wire(code.wire()), Some(*code));
        assert!(
            !wires.contains(&code.wire()),
            "{code:?} duplicates a registered import warning code"
        );
        wires.push(code.wire());
        match metadata.audience {
            Audience::Linguist => {
                let guidance = metadata
                    .guidance
                    .expect("every linguist import code has FieldWorks guidance");
                assert!(
                    guidance.contains(" > "),
                    "{code:?} guidance must use a FieldWorks path: {guidance}"
                );
            }
            Audience::Developer => {
                assert!(metadata.guidance.is_none(), "{code:?} is internal");
            }
        }
    }
    assert_eq!(wires.len(), ImportWarningCode::ALL.len());
}

#[test]
fn import_warning_guidance_uses_the_tool_that_edits_each_object() {
    let cases = [
        (
            ImportWarningCode::FwdataMissingLangProject,
            "File > Restore a Project...",
        ),
        (
            ImportWarningCode::FwdataWritingSystemStoreUnreadable,
            "Tools > Configure > Set up Vernacular Writing Systems...",
        ),
        (ImportWarningCode::FwdataOnlyFirstUsed, "Grammar > Phonemes"),
        (
            ImportWarningCode::FwdataMetathesisApproximation,
            "Grammar > Phonological Rules",
        ),
        (
            ImportWarningCode::SnapshotRuleFeatureUnresolved,
            "Grammar > Phonological Rules",
        ),
        (
            ImportWarningCode::RuleMetathesisUnsupported,
            "Grammar > Phonological Rules",
        ),
        (
            ImportWarningCode::RuleBuildFailed,
            "Grammar > Phonological Rules",
        ),
        (
            ImportWarningCode::FeatureConstraintUnresolved,
            "Grammar > Phonological Rules",
        ),
        (
            ImportWarningCode::FeatureConstraintPhonFeatureUnresolved,
            "Grammar > Phonological Rules",
        ),
        (
            ImportWarningCode::RuleFeatureUnresolved,
            "Grammar > Phonological Rules",
        ),
        (
            ImportWarningCode::MsaLexEntryInflTypeUnresolved,
            "Lists > Variant Types",
        ),
        (
            ImportWarningCode::NullAffixMprUnresolved,
            "Lists > Variant Types",
        ),
        (
            ImportWarningCode::NullAffixSynFsFailed,
            "Lists > Variant Types",
        ),
        (
            ImportWarningCode::NullAffixSegmentFailed,
            "Lists > Variant Types",
        ),
        (
            ImportWarningCode::StemNameBuildFailed,
            "Grammar > Category Edit",
        ),
        (
            ImportWarningCode::StemNameEmptyRegions,
            "Grammar > Category Edit",
        ),
        (
            ImportWarningCode::BoundaryMorphMarkerUnresolved,
            "Words > Edit Parser Parameters...",
        ),
        (
            ImportWarningCode::FwdataInvalidParserParameter,
            "Words > Edit Parser Parameters...",
        ),
        (
            ImportWarningCode::InvalidSourceActiveParser,
            "Words > Edit Parser Parameters...",
        ),
        (
            ImportWarningCode::StrataCustomUnsupported,
            "Words > Edit Parser Parameters...",
        ),
        (
            ImportWarningCode::AllomorphEnvironmentBuildFailed,
            "Lexicon > Lexicon Edit",
        ),
        (
            ImportWarningCode::CircumfixEnvironmentCombinationSkipped,
            "Lexicon > Lexicon Edit",
        ),
    ];

    for (code, expected_path) in cases {
        let metadata = pg_snapshot::import_warning_metadata(code);
        let guidance = metadata
            .guidance
            .unwrap_or_else(|| panic!("{code:?} must have guidance"));
        assert!(
            guidance.contains(expected_path),
            "{code:?} should guide to {expected_path}, got {guidance}"
        );
    }

    let environment_reference =
        pg_snapshot::import_warning_metadata(ImportWarningCode::EnvironmentUnresolved)
            .guidance
            .expect("unresolved environment guidance");
    assert!(environment_reference.contains("Grammar > Environments"));
    assert!(environment_reference.contains("Lexicon > Lexicon Edit"));
}
// --- hc-duplicate-feature-bundle ------------------------------------------------------

const TWO_SEGMENTS_SHARE_BUNDLE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>DuplicateBundle</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<PhonologicalFeatureSystem>
  <SymbolicFeature id="feat_voc"><Name>voc</Name>
    <Symbols><Symbol id="sym_p">+</Symbol><Symbol id="sym_m">-</Symbol></Symbols>
  </SymbolicFeature>
</PhonologicalFeatureSystem>
<CharacterDefinitionTable id="table1">
  <Name>table1</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
      <FeatureValue feature="feat_voc" symbolValues="sym_p" />
    </SegmentDefinition>
    <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
      <FeatureValue feature="feat_voc" symbolValues="sym_p" />
    </SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses></NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn two_segments_share_feature_bundle_reports_both_by_name() {
    let g = grammar(TWO_SEGMENTS_SHARE_BUNDLE_XML);
    let findings = check_grammar_health(&g, None).expect("grammar-health checks");
    assert_eq!(findings.len(), 1);
    let finding = &findings[0];
    assert_eq!(finding.code, GrammarHealthCode::DuplicateFeatureBundle);
    assert_eq!(finding.severity, GrammarHealthSeverity::Warning);
    assert_eq!(
        finding.message,
        "Phonemes a, b have identical feature values and cannot be distinguished by FieldWorks rules."
    );
    assert_eq!(
        finding.guidance.as_deref(),
        Some("In Grammar > Phonemes, assign distinct feature values to these phonemes.")
    );
    assert!(matches!(
        &finding.subjects[0],
        GrammarHealthSubject { kind: FwClass::PhPhonemeSet, title, .. } if title == "table1"
    ));
}

const DISTINCT_BUNDLES_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>DistinctBundles</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<PhonologicalFeatureSystem>
  <SymbolicFeature id="feat_voc"><Name>voc</Name>
    <Symbols><Symbol id="sym_p">+</Symbol><Symbol id="sym_m">-</Symbol></Symbols>
  </SymbolicFeature>
</PhonologicalFeatureSystem>
<CharacterDefinitionTable id="table1">
  <Name>table1</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
      <FeatureValue feature="feat_voc" symbolValues="sym_p" />
    </SegmentDefinition>
    <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
      <FeatureValue feature="feat_voc" symbolValues="sym_m" />
    </SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses></NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn every_segment_has_distinct_feature_bundle_no_findings() {
    let g = grammar(DISTINCT_BUNDLES_XML);
    assert!(check_grammar_health(&g, None)
        .expect("grammar-health checks")
        .is_empty());
}

const ZERO_FEATURE_SYSTEM_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>ZeroFeatureSystem</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table1">
  <Name>table1</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="char_c"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses></NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn no_phonological_feature_system_does_not_flag_trivially_identical_bundles() {
    // No `<PhonologicalFeatureSystem>` at all (the real Sena shape) -- every bundle is the same empty struct, so this must not report a duplicate.
    let g = grammar(ZERO_FEATURE_SYSTEM_XML);
    assert!(check_grammar_health(&g, None)
        .expect("grammar-health checks")
        .is_empty());
}

// --- hc-undeclared-segment -------------------------------------------------------------

const CLEAN_LEXICON_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>CleanLexicon</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table1">
  <Name>table1</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses></NaturalClasses>
<Strata>
  <Stratum characterDefinitionTable="table1">
    <Name>Surface</Name>
    <LexicalEntries>
      <LexicalEntry id="e1" partOfSpeech="posV">
        <Allomorphs><Allomorph id="a1"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn clean_grammar_no_findings_at_all() {
    let g = grammar(CLEAN_LEXICON_XML);
    assert!(check_grammar_health(&g, None)
        .expect("grammar-health checks")
        .is_empty());
}

/// A hand-built `Shape` bypassing the table's own validated segmentation -- mirrors C#'s own test note that direct object-model construction need not go through it.
fn undeclared_shape() -> Shape {
    let mut b = ShapeBuilder::new();
    b.push_segment(9_999);
    b.finish()
}

#[test]
fn lexical_entry_uses_segment_no_table_declares_reports_finding() {
    let mut g = grammar(CLEAN_LEXICON_XML);
    g.entries[0].allomorphs[0].shape.shape = undeclared_shape();

    let findings = check_grammar_health(&g, None).expect("grammar-health checks");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, GrammarHealthCode::UndeclaredSegment);
    assert_eq!(findings[0].severity, GrammarHealthSeverity::Error);
    assert_eq!(
        findings[0].message,
        "Lexical entry 'ab' uses a phoneme that is missing from the phoneme inventory."
    );
    assert_eq!(
        findings[0].guidance.as_deref(),
        Some("In Lexicon > Lexicon Edit, correct the form or add the missing phoneme in Grammar > Phonemes.")
    );
}

const AFFIX_INSERT_SEGMENTS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>AffixInsertSegments</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table1">
  <Name>table1</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses>
  <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
</NaturalClasses>
<Strata>
  <Stratum characterDefinitionTable="table1" morphologicalRules="mr1">
    <Name>Surface</Name>
    <MorphologicalRuleDefinitions>
      <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
        <Name>plural</Name>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="sub1">
            <MorphologicalInput>
              <PhoneticSequence id="stem">
                <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
              </PhoneticSequence>
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
      <LexicalEntry id="e1" partOfSpeech="posV">
        <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>
"#;

/// Finds the sole `InsertSegments` action inside `grammar.mrules[0]`'s single allomorph.
fn insert_segments_shape_mut(g: &mut Grammar) -> &mut Shape {
    let MorphRuleDef::AffixProcess(def) = &mut g.mrules[0] else {
        panic!("expected an AffixProcess rule");
    };
    for action in &mut def.allomorphs[0].rhs {
        if let OutputAction::InsertSegments { shape, .. } = action {
            return &mut shape.shape;
        }
    }
    panic!("expected an InsertSegments action");
}

#[test]
fn affix_process_rule_insert_segments_undeclared_reports_finding() {
    let mut g = grammar(AFFIX_INSERT_SEGMENTS_XML);
    *insert_segments_shape_mut(&mut g) = undeclared_shape();

    let findings = check_grammar_health(&g, None).expect("grammar-health checks");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, GrammarHealthCode::UndeclaredSegment);
    assert_eq!(
        findings[0].message,
        "Inflectional affix 'plural' uses a phoneme that is missing from the phoneme inventory."
    );
    assert_eq!(
        findings[0].guidance.as_deref(),
        Some("In Lexicon > Lexicon Edit, correct the affix form or add the missing phoneme in Grammar > Phonemes.")
    );
}

#[test]
fn affix_description_uses_the_source_msa_class() {
    for (class, label) in [
        (FwClass::MoDerivAffMsa, "Derivational affix"),
        (FwClass::MoUnclassifiedAffixMsa, "Unclassified affix"),
    ] {
        let mut g = grammar(AFFIX_INSERT_SEGMENTS_XML);
        *insert_segments_shape_mut(&mut g) = undeclared_shape();
        let MorphRuleDef::AffixProcess(def) = &g.mrules[0] else {
            panic!("expected an AffixProcess rule");
        };
        g.morphemes[def.morpheme.0 as usize].source_msa_class = Some(class);

        let findings = check_grammar_health(&g, None).expect("grammar-health checks");
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].message,
            format!("{label} 'plural' uses a phoneme that is missing from the phoneme inventory.")
        );
        assert_eq!(findings[0].subjects[1].kind, class);
    }
}

const COMPOUNDING_INSERT_SEGMENTS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>CompoundingInsertSegments</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table1">
  <Name>table1</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
  <BoundaryDefinitions>
    <BoundaryDefinition id="char_bnd"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
  </BoundaryDefinitions>
</CharacterDefinitionTable>
<NaturalClasses>
  <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
</NaturalClasses>
<Strata>
  <Stratum characterDefinitionTable="table1" morphologicalRules="mrC">
    <Name>Surface</Name>
    <MorphologicalRuleDefinitions>
      <CompoundingRule id="mrC">
        <Name>compound1</Name>
        <CompoundingSubrules><CompoundingSubrule>
          <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
          <NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput>
        </CompoundingSubrule></CompoundingSubrules>
      </CompoundingRule>
    </MorphologicalRuleDefinitions>
    <LexicalEntries>
      <LexicalEntry id="e1" partOfSpeech="posV">
        <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn compounding_rule_insert_segments_undeclared_reports_finding() {
    let mut g = grammar(COMPOUNDING_INSERT_SEGMENTS_XML);
    let MorphRuleDef::Compounding(def) = &mut g.mrules[0] else {
        panic!("expected a Compounding rule");
    };
    let mut replaced = false;
    for action in &mut def.subrules[0].rhs {
        if let OutputAction::InsertSegments { shape, .. } = action {
            shape.shape = undeclared_shape();
            replaced = true;
        }
    }
    assert!(replaced, "fixture must contain an InsertSegments action");

    let findings = check_grammar_health(&g, None).expect("grammar-health checks");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, GrammarHealthCode::UndeclaredSegment);
    assert_eq!(
        findings[0].message,
        "Compound rule 'compound1' uses a phoneme that is missing from the phoneme inventory."
    );
    assert_eq!(
        findings[0].guidance.as_deref(),
        Some("In Grammar > Compound Rules, correct the rule or add the missing phoneme in Grammar > Phonemes.")
    );
}

// --- hc-partial-morpheme -----------------------------------------------------------------

const PARTIAL_LEX_ENTRY_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>PartialLexEntry</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table1">
  <Name>table1</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses></NaturalClasses>
<Strata>
  <Stratum characterDefinitionTable="table1">
    <Name>Surface</Name>
    <LexicalEntries>
      <LexicalEntry id="entry1" partial="true">
        <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn partial_lexical_entry_reports_actionable_warning() {
    let g = grammar(PARTIAL_LEX_ENTRY_XML);
    let findings = check_grammar_health(&g, None).expect("grammar-health checks");
    assert_eq!(findings.len(), 1);
    let finding = &findings[0];
    assert_eq!(finding.code.wire(), "hc-stem-no-grammatical-category");
    assert_eq!(finding.severity, GrammarHealthSeverity::Warning);
    assert_eq!(
        finding.message,
        "Lexical entry 'a' has no grammatical category."
    );
    assert_eq!(
        finding.guidance.as_deref(),
        Some("In Lexicon > Lexicon Edit, open the entry and set Grammatical Info. > Category.")
    );
    assert!(matches!(
        &finding.subjects[..],
        [GrammarHealthSubject { kind: FwClass::MoForm, title, .. }] if title == "a"
    ));
}

const PARTIAL_TEMPLATE_RULE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>PartialTemplateRule</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table1">
  <Name>table1</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses>
  <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
</NaturalClasses>
<Strata>
  <Stratum characterDefinitionTable="table1">
    <Name>Surface</Name>
    <MorphologicalRuleDefinitions>
      <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV" partial="true">
        <Name>subject</Name>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="sub1">
            <MorphologicalInput>
              <PhoneticSequence id="stem">
                <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
              </PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="stem" /></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    </MorphologicalRuleDefinitions>
    <AffixTemplates>
      <AffixTemplate>
        <Name>verb1</Name>
        <Slot morphologicalRules="mr1"><Name>Sl1</Name></Slot>
      </AffixTemplate>
      <AffixTemplate>
        <Name>verb2</Name>
        <Slot morphologicalRules="mr1"><Name>Sl2</Name></Slot>
      </AffixTemplate>
    </AffixTemplates>
    <LexicalEntries>
      <LexicalEntry id="e1" partOfSpeech="posV">
        <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn partial_ordinary_rule_reports_rule() {
    // Distinct from the template-only fixture below: this one lists `mr1` in the stratum's own `morphologicalRules`, exercising the ordinary-rule path.
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>PartialOrdinaryRule</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table1">
  <Name>table1</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses>
  <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="char_a" /></SegmentNaturalClass>
</NaturalClasses>
<Strata>
  <Stratum characterDefinitionTable="table1" morphologicalRules="mr1">
    <Name>Surface</Name>
    <MorphologicalRuleDefinitions>
      <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV" partial="true">
        <Name>plural</Name>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="sub1">
            <MorphologicalInput>
              <PhoneticSequence id="stem">
                <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
              </PhoneticSequence>
            </MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="stem" /></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    </MorphologicalRuleDefinitions>
    <LexicalEntries>
      <LexicalEntry id="e1" partOfSpeech="posV">
        <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>
"#;
    let g = grammar(XML);
    let findings = check_grammar_health(&g, None).expect("grammar-health checks");
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].code,
        GrammarHealthCode::InflectionalAffixWithoutTemplateSlot
    );
    assert!(findings[0].message.contains("plural"));
    assert!(matches!(
        &findings[0].subjects[..],
        [GrammarHealthSubject { kind: FwClass::MoInflAffMsa, title, .. }] if title == "plural"
    ));
}

#[test]
fn partial_template_rule_referenced_twice_reports_once() {
    let g = grammar(PARTIAL_TEMPLATE_RULE_XML);
    let findings = check_grammar_health(&g, None).expect("grammar-health checks");
    assert_eq!(findings.len(), 1, "referenced by two slots, reported once");
    assert_eq!(
        findings[0].code.wire(),
        "hc-inflectional-affix-missing-template-slot"
    );
    assert_eq!(
        findings[0].message,
        "Inflectional affix 'subject' has no template slot."
    );
    assert_eq!(
        findings[0].guidance.as_deref(),
        Some("In Grammar > Category Edit > the category's Affix Templates, assign the affix to a template slot.")
    );
}

const PARTIAL_MORPHEME_AND_EXISTING_PROBLEM_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>PartialAndDuplicate</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<PhonologicalFeatureSystem>
  <SymbolicFeature id="feat_voc"><Name>voc</Name>
    <Symbols><Symbol id="sym_p">+</Symbol><Symbol id="sym_m">-</Symbol></Symbols>
  </SymbolicFeature>
</PhonologicalFeatureSystem>
<CharacterDefinitionTable id="table1">
  <Name>table1</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
      <FeatureValue feature="feat_voc" symbolValues="sym_p" />
    </SegmentDefinition>
    <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
      <FeatureValue feature="feat_voc" symbolValues="sym_p" />
    </SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses></NaturalClasses>
<Strata>
  <Stratum characterDefinitionTable="table1">
    <Name>Surface</Name>
    <LexicalEntries>
      <LexicalEntry id="entry1" partial="true">
        <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn partial_morpheme_and_existing_problem_reports_both() {
    let g = grammar(PARTIAL_MORPHEME_AND_EXISTING_PROBLEM_XML);
    let mut found = codes(&check_grammar_health(&g, None).expect("grammar-health checks"));
    found.sort_by_key(|c| c.wire().to_string());
    let mut expected = vec![
        GrammarHealthCode::DuplicateFeatureBundle,
        GrammarHealthCode::StemWithoutCategory,
    ];
    expected.sort_by_key(|c| c.wire().to_string());
    assert_eq!(found, expected);
}

// --- serialization ------------------------------------------------------------------------

#[test]
fn report_round_trips_through_the_single_decode_path() {
    let g = grammar(PARTIAL_LEX_ENTRY_XML);
    let report = check_grammar_health(&g, None).expect("grammar-health checks");
    let json = report.to_json().expect("report must serialize");
    assert!(json.contains("hc-stem-no-grammatical-category"));
    let round_tripped = GrammarHealthReport::from_json(&json).expect("report must decode");
    assert_eq!(round_tripped, report);
}

#[test]
fn every_code_serializes_to_its_stable_wire_string() {
    for (code, wire) in [
        (
            GrammarHealthCode::UndeclaredSegment,
            "hc-undeclared-segment",
        ),
        (
            GrammarHealthCode::DuplicateFeatureBundle,
            "hc-duplicate-feature-bundle",
        ),
        (
            GrammarHealthCode::StemWithoutCategory,
            "hc-stem-no-grammatical-category",
        ),
        (
            GrammarHealthCode::InflectionalAffixWithoutTemplateSlot,
            "hc-inflectional-affix-missing-template-slot",
        ),
    ] {
        assert_eq!(code.wire(), wire);
        assert_eq!(serde_json::to_string(&code).unwrap(), format!("{wire:?}"));
    }
}

#[test]
fn group_names_describe_their_codes() {
    assert_eq!(
        GrammarHealthCode::UndeclaredSegment.group_name(),
        "Missing segment definition"
    );
    assert_eq!(
        GrammarHealthCode::DuplicateFeatureBundle.group_name(),
        "Duplicate segment features"
    );
    assert_eq!(
        GrammarHealthCode::StemWithoutCategory.group_name(),
        "Stem has no category"
    );
    assert_eq!(
        GrammarHealthCode::InflectionalAffixWithoutTemplateSlot.group_name(),
        "Inflectional affix has no slot"
    );
}

const FIELDWORKS_GUID_PARTIAL_XML: &str = r#"<HermitCrabInput><Language>
<Name>FieldWorks Demo</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table1"><Name>Orthography</Name>
<SegmentDefinitions><SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
</CharacterDefinitionTable>
<Strata><Stratum characterDefinitionTable="table1"><Name>main</Name><LexicalEntries>
<LexicalEntry id="f4e4b416-5a15-41e3-9039-c3cca7093153" partial="true">
<MorphemeId>walk</MorphemeId><Gloss>walk</Gloss>
<Allomorphs><Allomorph id="allo1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
</LexicalEntry>
</LexicalEntries></Stratum></Strata>
</Language></HermitCrabInput>"#;

#[test]
fn fieldworks_guid_partial_entry_uses_its_readable_name() {
    let mut g = grammar(FIELDWORKS_GUID_PARTIAL_XML);
    g.allomorph_sources[0].form_guids =
        vec![Some("f4e4b416-5a15-41e3-9039-c3cca7093153".to_string())];
    let findings =
        check_grammar_health(&g, Some("FieldWorks Demo")).expect("grammar-health checks");
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].message,
        "Lexical entry 'a' (walk) has no grammatical category."
    );
    assert_eq!(
        findings[0].guidance.as_deref(),
        Some("In Lexicon > Lexicon Edit, open the entry and set Grammatical Info. > Category.")
    );
    assert!(matches!(
        &findings[0].subjects[..],
        [GrammarHealthSubject {
            kind: FwClass::MoForm,
            title,
            subtitle: Some(subtitle),
            fieldworks: FieldWorksLink::Available { guid, tool, .. },
            ..
        }] if title == "a" && subtitle == "walk"
            && guid == "f4e4b416-5a15-41e3-9039-c3cca7093153"
            && tool == "lexiconEdit"
    ));
    assert!(!findings[0]
        .message
        .contains("f4e4b416-5a15-41e3-9039-c3cca7093153"));
}

fn assert_no_blank_or_internal_subjects(report: &GrammarHealthReport) {
    let findings = report.findings();
    for finding in findings {
        assert!(!finding.message.trim().is_empty());
        assert!(!finding.code.wire().is_empty());
        for subject in &finding.subjects {
            assert!(!subject_kind_label(subject.kind).is_empty());
            assert!(!subject.title.trim().is_empty());
            assert!(!subject.render_location(false).trim().is_empty());
            assert!(!is_internal_subject_label(&subject.title));
        }
        let json = serde_json::to_value(finding).expect("finding serializes");
        assert!(json["severity"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
        assert_eq!(json["group_name"], finding.code.group_name());
        assert!(json["description"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
        let subjects = json["subjects"]
            .as_array()
            .expect("subjects serialize as an array");
        assert!(!subjects.is_empty());
        for subject in subjects {
            assert!(subject["kind"]
                .as_str()
                .is_some_and(|value| !value.is_empty()));
            assert!(subject["title"]
                .as_str()
                .is_some_and(|value| !value.trim().is_empty()));
        }
    }
}

#[test]
fn report_rejects_an_incomplete_finding_instead_of_dropping_it() {
    let incomplete = GrammarHealthCheckFinding {
        severity: GrammarHealthSeverity::Warning,
        code: GrammarHealthCode::StemWithoutCategory,
        group_name: "Stem has no category".to_string(),
        origin: FindingOrigin::Check,
        audience: Audience::Linguist,
        message: " ".to_string(),
        guidance: None,
        subjects: Vec::new(),
    };
    let error = GrammarHealthReport::new(vec![incomplete])
        .expect_err("incomplete findings must fail report construction");
    assert_eq!(error.code, GrammarHealthReportErrorCode::InvalidFinding);
    assert_eq!(error.finding_index, Some(0));
    assert_eq!(error.field.as_deref(), Some("description"));
}

#[test]
fn validated_empty_report_renders_lossless_log() {
    let report = GrammarHealthReport::new(Vec::new()).expect("empty report is valid");
    assert_eq!(render_log(&report, false), "");
}

#[test]
fn an_empty_report_remains_valid() {
    let report = GrammarHealthReport::new(Vec::new()).expect("empty report is valid");
    assert_eq!(render_log(&report, false), "");
    let json = render_json(&report).expect("empty report serializes");
    let report: serde_json::Value = serde_json::from_str(&json).expect("versioned report");
    assert_eq!(report["schema_version"], GRAMMAR_HEALTH_SCHEMA_VERSION);
    assert_eq!(report["findings"], serde_json::json!([]));
}

#[test]
fn every_code_variant_occurs_once_in_all() {
    assert_eq!(GrammarHealthCode::ALL.len(), 5);
    for code in [
        GrammarHealthCode::UndeclaredSegment,
        GrammarHealthCode::DuplicateFeatureBundle,
        GrammarHealthCode::StemWithoutCategory,
        GrammarHealthCode::InflectionalAffixWithoutTemplateSlot,
    ] {
        assert_eq!(
            GrammarHealthCode::ALL
                .iter()
                .filter(|candidate| **candidate == code)
                .count(),
            1
        );
    }
}

#[test]
fn internal_hash_identifiers_are_not_accepted_as_human_titles() {
    assert!(is_internal_subject_label("mrule#18"));
    assert!(is_internal_subject_label("lex_entry#4:entry"));
}

#[test]
fn authored_names_resembling_ids_are_accepted_as_titles() {
    for authored in [
        "rule1",
        "slot2",
        "entry3",
        "Rule #1",
        "0a1b2c3d-0000-0000-0000-000000000000",
    ] {
        assert!(!is_internal_subject_label(authored), "{authored}");
    }
}

#[test]
fn direct_report_decode_rejects_an_unsupported_schema_version() {
    let json = serde_json::json!({
        "schema_version": 99,
        "findings": [],
    });
    let error = GrammarHealthReport::from_json(&json.to_string())
        .expect_err("unsupported schema versions must be rejected");
    assert_eq!(
        error.code,
        GrammarHealthReportErrorCode::UnsupportedSchemaVersion
    );
    assert_eq!(error.field.as_deref(), Some("schema_version"));
}

#[test]
fn direct_report_decode_rejects_both_fieldworks_link_states() {
    let grammar = grammar(FIELDWORKS_GUID_PARTIAL_XML);
    let source_report = check_grammar_health(&grammar, None).expect("grammar-health checks");
    let finding = source_report
        .findings()
        .iter()
        .next()
        .expect("fixture has a finding");
    let report = GrammarHealthReport::new(vec![finding.clone()]).expect("report validates");
    let mut json = serde_json::to_value(&report).expect("report serializes");
    json["findings"][0]["subjects"][0]["fieldworks"] = serde_json::json!({
        "status": "available",
        "guid": "invalid",
        "tool": "lexiconEdit",
        "url": "silfw://invalid"
    });
    let error = GrammarHealthReport::from_json(&json.to_string())
        .expect_err("invalid link state must be rejected");
    assert_eq!(error.field.as_deref(), Some("fieldworks.guid"));
}

#[test]
fn canonical_report_round_trips_without_changing_consumer_values() {
    let findings = check_grammar_health(&grammar(FIELDWORKS_GUID_PARTIAL_XML), None)
        .expect("grammar-health checks");
    let original = findings;
    let json = original.to_json().expect("canonical report serializes");
    let decoded = GrammarHealthReport::from_json(&json).expect("canonical report decodes");
    assert_eq!(decoded, original);
}

#[test]
fn every_existing_grammar_fixture_has_complete_human_findings() {
    let mut compounding_grammar = grammar(COMPOUNDING_INSERT_SEGMENTS_XML);
    let MorphRuleDef::Compounding(def) = &mut compounding_grammar.mrules[0] else {
        panic!("expected a Compounding rule");
    };
    for action in &mut def.subrules[0].rhs {
        if let OutputAction::InsertSegments { shape, .. } = action {
            shape.shape = undeclared_shape();
        }
    }
    let compounding_findings = check_grammar_health(&compounding_grammar, Some("FieldWorks Demo"))
        .expect("grammar-health checks");
    let compounding_subject = compounding_findings
        .iter()
        .flat_map(|finding| &finding.subjects)
        .find(|subject| subject.kind == FwClass::MoCompoundRule)
        .expect("compounding finding names its rule");
    assert!(matches!(
        compounding_subject.fieldworks,
        FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::UnsupportedKind,
            ..
        }
    ));
    for xml in [
        TWO_SEGMENTS_SHARE_BUNDLE_XML,
        DISTINCT_BUNDLES_XML,
        ZERO_FEATURE_SYSTEM_XML,
        CLEAN_LEXICON_XML,
        AFFIX_INSERT_SEGMENTS_XML,
        COMPOUNDING_INSERT_SEGMENTS_XML,
        PARTIAL_LEX_ENTRY_XML,
        PARTIAL_TEMPLATE_RULE_XML,
        PARTIAL_MORPHEME_AND_EXISTING_PROBLEM_XML,
        FIELDWORKS_GUID_PARTIAL_XML,
    ] {
        let findings = check_grammar_health(&grammar(xml), None).expect("grammar-health checks");
        assert_no_blank_or_internal_subjects(&findings);
    }
}

#[test]
fn log_and_json_render_the_same_guid_fixture_with_explicit_link_state() {
    let mut g = grammar(FIELDWORKS_GUID_PARTIAL_XML);
    let allomorph_id = g.entries[0].allomorphs[0].id.0 as usize;
    g.allomorph_sources[allomorph_id].form_guids =
        vec![Some("f4e4b416-5a15-41e3-9039-c3cca7093153".to_string())];
    let with_project =
        check_grammar_health(&g, Some("FieldWorks Demo")).expect("grammar-health checks");
    assert_no_blank_or_internal_subjects(&with_project);
    let subject = &with_project[0].subjects[0];
    assert_eq!(subject.title, "a");
    assert_eq!(subject.subtitle.as_deref(), Some("walk"));
    let FieldWorksLink::Available { guid, tool, url } = &subject.fieldworks else {
        panic!("provenance-backed lexical subject should have a link");
    };
    assert_eq!(guid, "f4e4b416-5a15-41e3-9039-c3cca7093153");
    assert_eq!(tool, "lexiconEdit");
    assert!(url.contains("database%3DFieldWorks+Demo%26tool%3DlexiconEdit%26guid%3Df4e4b416-5a15-41e3-9039-c3cca7093153%26tag%3D"));
    assert!(!url.contains("&tool="));
    assert!(!url.contains("&guid="));

    let log = render_log(&with_project, false);
    assert_eq!(log.lines().count(), with_project.len());
    assert!(log.lines().all(|line| !line.trim().is_empty()));
    assert!(log.contains("a (walk)"));
    assert!(log.contains("Stem has no category"));
    assert!(log.contains(url));
    assert!(!log.contains("entry0"));
    let log_with_guids = render_log(&with_project, true);
    assert!(log_with_guids.contains("a (walk) [guid f4e4b416-5a15-41e3-9039-c3cca7093153]"));
    assert!(!log_with_guids.contains("entry0"));

    let json = render_json(&with_project).expect("structured findings serialize");
    assert!(json.contains("\"schema_version\": 2"));
    assert!(json.contains("\"group_name\": \"Stem has no category\""));
    assert!(json.contains("silfw://localhost/link?database%3DFieldWorks+Demo%26tool%3DlexiconEdit"));
    assert!(json.contains("\"internal_id\""));

    let without_project = check_grammar_health(&g, None).expect("grammar-health checks");
    let no_link = &without_project[0].subjects[0].fieldworks;
    assert!(matches!(
        no_link,
        FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::MissingProject,
            guid: Some(_),
        }
    ));
    let no_project_json = render_json(&without_project).expect("structured findings serialize");
    assert!(no_project_json.contains("missing_project"));
    assert!(no_project_json.contains("f4e4b416-5a15-41e3-9039-c3cca7093153"));
    assert!(!no_project_json.contains("silfw://localhost/link?database="));

    let no_guid_findings = check_grammar_health(
        &grammar(TWO_SEGMENTS_SHARE_BUNDLE_XML),
        Some("FieldWorks Demo"),
    )
    .expect("grammar-health checks");
    let no_guid_log = render_log(&no_guid_findings, true);
    assert!(no_guid_log.contains("source item has no FieldWorks GUID"));
    assert!(no_guid_log.contains("[guid unavailable]"));
}

#[test]
fn log_rendering_keeps_subject_context_and_navigation_state() {
    let duplicate = check_grammar_health(
        &grammar(TWO_SEGMENTS_SHARE_BUNDLE_XML),
        Some("FieldWorks Demo"),
    )
    .expect("duplicate fixture checks");
    let log = render_log(&duplicate, false);
    assert!(log.contains("in table1"), "{log}");
    assert!(log.contains("FieldWorks link unavailable:"), "{log}");
    assert!(log.contains("source item has no FieldWorks GUID"), "{log}");
}

#[test]
fn partial_warning_subjects_match_canonical_facts_in_both_directions() {
    for xml in [
        PARTIAL_MORPHEME_AND_EXISTING_PROBLEM_XML,
        PARTIAL_TEMPLATE_RULE_XML,
    ] {
        assert_partial_subjects_match_facts(&grammar(xml));
    }
}

fn assert_partial_subjects_match_facts(g: &Grammar) {
    let facts = g.partial_morpheme_facts().expect("valid partial facts");
    assert!(
        facts.has_partials(),
        "fixture must declare a partial morpheme"
    );
    let findings = check_grammar_health(g, None).expect("grammar-health checks");
    let mut warning_subjects = findings
        .iter()
        .filter(|finding| {
            matches!(
                &finding.code,
                GrammarHealthCode::StemWithoutCategory
                    | GrammarHealthCode::InflectionalAffixWithoutTemplateSlot
            )
        })
        .flat_map(|finding| finding.subjects.iter())
        .map(|subject| {
            (
                subject.kind,
                subject.title.clone(),
                subject.internal_id.clone().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    let mut fact_subjects = facts
        .identities()
        .map(|identity| match identity {
            PartialMorphemeIdentity::LexicalEntry { id, .. } => (
                fieldworks_identity(g, &FieldWorksSource::LexEntry(*id)).class,
                g.lex_entry_display_parts(*id).expect("entry title").0,
                g.lex_entry_internal_id(*id).expect("entry identity"),
            ),
            PartialMorphemeIdentity::MorphologicalRule { id, .. } => (
                fieldworks_identity(g, &FieldWorksSource::MorphRule(*id)).class,
                g.morph_rule_display_parts(*id).expect("rule title").0,
                g.morph_rule_internal_id(*id).expect("rule identity"),
            ),
        })
        .collect::<Vec<_>>();
    let sort_subjects = |subjects: &mut Vec<(FwClass, String, String)>| {
        subjects.sort_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then_with(|| left.2.cmp(&right.2))
                .then_with(|| subject_kind_label(left.0).cmp(subject_kind_label(right.0)))
        });
    };
    sort_subjects(&mut warning_subjects);
    sort_subjects(&mut fact_subjects);
    assert_eq!(warning_subjects, fact_subjects);
    for (_, _, internal_id) in &warning_subjects {
        assert_eq!(internal_id.matches('#').count(), 1, "{internal_id}");
    }
}

#[test]
fn rule_subject_internal_id_is_the_canonical_model_id() {
    let g = grammar(AFFIX_INSERT_SEGMENTS_XML);
    let subject = morph_rule_subject(&g, None, MRuleId(0)).expect("rule subject");
    assert_eq!(
        subject.internal_id,
        Some(g.morph_rule_internal_id(MRuleId(0)).expect("model id"))
    );
    assert!(
        subject
            .internal_id
            .as_deref()
            .expect("compiled rule id")
            .starts_with("morph_rule#0"),
        "{}",
        subject.internal_id.as_deref().unwrap_or_default()
    );
    assert_eq!(
        subject.internal_id.as_deref().unwrap().matches('#').count(),
        1,
        "{}",
        subject.internal_id.as_deref().unwrap_or_default()
    );
}

#[test]
fn partial_fact_failure_is_propagated_instead_of_admitted() {
    let mut g = grammar(PARTIAL_TEMPLATE_RULE_XML);
    let MorphRuleDef::AffixProcess(def) = &mut g.mrules[0] else {
        panic!("fixture must contain an affix-process rule");
    };
    def.morpheme = crate::model::MorphemeId(u32::MAX);
    let error = check_grammar_health(&g, None).expect_err("invalid partial facts must fail");
    assert!(error.to_string().contains("unknown morpheme"), "{error}");
}

#[test]
fn invalid_subject_references_return_named_errors_instead_of_panicking() {
    let mut lexical = grammar(FIELDWORKS_GUID_PARTIAL_XML);
    lexical.entries[0].morpheme = crate::model::MorphemeId(u32::MAX);
    let error = lex_entry_subject(&lexical, None, LexEntryId(0))
        .expect_err("invalid lexical subject reference must fail");
    assert!(
        error.to_string().contains("morpheme id 4294967295"),
        "{error}"
    );

    let mut rule = grammar(AFFIX_INSERT_SEGMENTS_XML);
    let MorphRuleDef::AffixProcess(def) = &mut rule.mrules[0] else {
        panic!("fixture must contain an affix-process rule");
    };
    def.morpheme = crate::model::MorphemeId(u32::MAX);
    let error = morph_rule_subject(&rule, None, MRuleId(0))
        .expect_err("invalid morphological subject reference must fail");
    assert!(
        error.to_string().contains("morpheme id 4294967295"),
        "{error}"
    );
}

#[test]
fn fieldworks_navigation_has_exact_supported_and_unavailable_states() {
    // Proof citations: FieldWorks/Src/xWorks/FwLinkArgs.cs; RecordClerk.cs:1036, 998-1016; RecordList.cs:3435.
    let mut lexical_grammar = grammar(FIELDWORKS_GUID_PARTIAL_XML);
    let allomorph_id = lexical_grammar.entries[0].allomorphs[0].id.0 as usize;
    lexical_grammar.allomorph_sources[allomorph_id].form_guids =
        vec![Some("f4e4b416-5a15-41e3-9039-c3cca7093153".to_string())];
    let lexical =
        check_grammar_health(&lexical_grammar, Some("Demo")).expect("lexical fixture checks");
    let lexical = &lexical[0].subjects[0].fieldworks;
    assert!(matches!(
        lexical,
        FieldWorksLink::Available { guid, tool, url }
            if guid == "f4e4b416-5a15-41e3-9039-c3cca7093153"
                && tool == "lexiconEdit"
                && url == "silfw://localhost/link?database%3DDemo%26tool%3DlexiconEdit%26guid%3Df4e4b416-5a15-41e3-9039-c3cca7093153%26tag%3D"
    ));

    let mut affix = grammar(AFFIX_INSERT_SEGMENTS_XML);
    *insert_segments_shape_mut(&mut affix) = undeclared_shape();
    let MorphRuleDef::AffixProcess(def) = &affix.mrules[0] else {
        panic!("affix fixture checks");
    };
    affix.morphemes[def.morpheme.0 as usize].source_msa_guid =
        Some("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".to_string());
    affix.morphemes[def.morpheme.0 as usize].source_msa_class = Some(FwClass::MoInflAffMsa);
    let affix = check_grammar_health(&affix, Some("Demo")).expect("affix fixture checks");
    let affix = affix[0]
        .subjects
        .iter()
        .find(|subject| subject.kind == FwClass::MoInflAffMsa)
        .expect("affix subject")
        .fieldworks
        .clone();
    assert!(matches!(
        affix,
        FieldWorksLink::Available { guid, tool, url }
            if guid == "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
                && tool == "lexiconEdit"
                && url.contains("tool%3DlexiconEdit")
    ));

    let mut infl_type = grammar(AFFIX_INSERT_SEGMENTS_XML);
    *insert_segments_shape_mut(&mut infl_type) = undeclared_shape();
    let MorphRuleDef::AffixProcess(def) = &infl_type.mrules[0] else {
        panic!("infl-type fixture checks");
    };
    let info = &mut infl_type.morphemes[def.morpheme.0 as usize];
    info.source_msa_guid = None;
    info.source_msa_class = None;
    info.source_infl_type_guid = Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".to_string());
    let infl_type =
        check_grammar_health(&infl_type, Some("Demo")).expect("infl-type fixture checks");
    let infl_type = infl_type[0]
        .subjects
        .iter()
        .find(|subject| subject.kind == FwClass::LexEntryInflType)
        .expect("infl-type subject")
        .fieldworks
        .clone();
    assert!(matches!(
        infl_type,
        FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::UnsupportedKind,
            guid: Some(guid),
        } if guid == "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
    ));

    let mut compound = grammar(COMPOUNDING_INSERT_SEGMENTS_XML);
    let MorphRuleDef::Compounding(def) = &mut compound.mrules[0] else {
        panic!("fixture must contain a compounding rule");
    };
    for action in &mut def.subrules[0].rhs {
        if let OutputAction::InsertSegments { shape, .. } = action {
            shape.shape = undeclared_shape();
        }
    }
    let compound = check_grammar_health(&compound, Some("Demo")).expect("compound fixture checks");
    let compound = compound[0]
        .subjects
        .iter()
        .find(|subject| subject.kind == FwClass::MoCompoundRule)
        .expect("compound subject")
        .fieldworks
        .clone();
    assert!(matches!(
        compound,
        FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::UnsupportedKind,
            ..
        }
    ));

    let duplicate = check_grammar_health(&grammar(TWO_SEGMENTS_SHARE_BUNDLE_XML), Some("Demo"))
        .expect("table fixture checks");
    for (kind, reason) in [
        (
            FwClass::PhPhonemeSet,
            FieldWorksUnavailableReason::UnsupportedKind,
        ),
        (
            FwClass::PhPhoneme,
            FieldWorksUnavailableReason::GuidNotRecorded,
        ),
    ] {
        let link = duplicate[0]
            .subjects
            .iter()
            .find(|subject| subject.kind == kind)
            .expect("character-definition subject")
            .fieldworks
            .clone();
        assert!(matches!(
            link,
            FieldWorksLink::Unavailable {
                reason: actual_reason,
                ..
            } if actual_reason == reason
        ));
    }
}

#[test]
fn fieldworks_link_resolution_checks_kind_before_guid() {
    let unsupported = fieldworks_link_from_identity(FwClass::Project, None, Some("Demo"));
    assert!(matches!(
        unsupported,
        FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::UnsupportedKind,
            guid: None,
        }
    ));

    let phoneme = fieldworks_link_from_identity(
        FwClass::PhPhoneme,
        Some("12345678-1234-1234-1234-123456789abc"),
        Some("Demo"),
    );
    assert!(matches!(
        phoneme,
        FieldWorksLink::Available { tool, .. } if tool == "phonemeEdit"
    ));

    let invalid = fieldworks_link_from_identity(FwClass::PhPhoneme, Some("not-a-guid"), None);
    assert!(matches!(
        invalid,
        FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::InvalidGuid,
            guid: Some(guid),
        } if guid == "not-a-guid"
    ));
}

#[test]
fn phoneme_link_uses_verified_phoneme_edit_tool() {
    assert!(matches!(
        fieldworks_link_from_identity(
            FwClass::PhPhoneme,
            Some("12345678-1234-1234-1234-123456789abc"),
            Some("Demo"),
        ),
        FieldWorksLink::Available { tool, .. } if tool == "phonemeEdit"
    ));
}

#[test]
fn phoneme_source_identity_keeps_imported_guid_for_navigation() {
    let guid = "12345678-1234-1234-1234-123456789abc";
    let mut g = grammar(TWO_SEGMENTS_SHARE_BUNDLE_XML);
    g.char_tables[0] = CharDefTable::from_raw(
        "table1".to_string(),
        Some("Phonemes".to_string()),
        vec![crate::chardef::RawCharDef {
            xml_id: "phoneme-a".to_string(),
            source_guid: Some(guid.to_string()),
            kind: CharDefKind::Segment,
            representations: vec!["a".to_string()],
            feature_values: vec![],
        }],
        &g.phon_features,
    )
    .expect("synthetic phoneme table");

    let source = fieldworks_identity(&g, &FieldWorksSource::CharDef(TableId(0), CharDefId(0)));
    assert_eq!(source.class, FwClass::PhPhoneme);
    assert_eq!(source.guid.as_deref(), Some(guid));
    assert!(matches!(
        fieldworks_link_from_identity(source.class, source.guid.as_deref(), Some("Demo")),
        FieldWorksLink::Available { tool, .. } if tool == "phonemeEdit"
    ));
}

#[test]
fn report_constructor_owns_validation_and_renderers_accept_only_reports() {
    let finding = GrammarHealthCheckFinding {
        severity: GrammarHealthSeverity::Warning,
        code: GrammarHealthCode::StemWithoutCategory,
        group_name: "Stem has no category".to_string(),
        origin: FindingOrigin::Check,
        audience: Audience::Linguist,
        message: "partial".to_string(),
        guidance: None,
        subjects: vec![],
    };
    let report =
        GrammarHealthReport::new(vec![finding]).expect("project findings may have no subjects");
    assert_eq!(report.len(), 1);
}

#[test]
fn authored_xml_guid_shape_does_not_create_a_fieldworks_link() {
    let report = check_grammar_health(&grammar(FIELDWORKS_GUID_PARTIAL_XML), Some("Demo"))
        .expect("grammar-health checks");
    assert!(matches!(
        &report.findings()[0].subjects[0].fieldworks,
        FieldWorksLink::Unavailable {
            reason: FieldWorksUnavailableReason::GuidNotRecorded,
            ..
        }
    ));
}
