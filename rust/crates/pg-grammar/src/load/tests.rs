use super::*;
use std::path::{Path, PathBuf};

/// Same self-skipping corpus-file locator as the lib.rs segmentation tests: `*-hc.xml` grammars are untracked local corpus files. Returns `None` if absent so callers self-skip.
fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn pos_symbol_count(g: &Grammar) -> usize {
    match &g.syn_features.features[g.syn_features.pos.0 as usize].kind {
        SynFeatureKind::Symbolic { symbols, .. } => symbols.len(),
        SynFeatureKind::Complex => 0,
    }
}

/// Load a real grammar and assert exact structural counts; every expected count was obtained independently of this loader by grepping the raw XML, so a match is real evidence, not an echo of its own output.
#[allow(clippy::too_many_arguments)]
fn check(
    xml_name: &str,
    expect_syn_features: usize,
    expect_pos: usize,
    expect_nat_classes: usize,
    expect_prules: usize,
    expect_mrules: usize,
    expect_templates: usize,
    expect_entries: usize,
    expect_strata: usize,
) {
    let Some(path) = sample_path(xml_name) else {
        eprintln!("skipping {xml_name}: sample grammar not present on disk");
        return;
    };
    let xml = std::fs::read_to_string(&path).expect("read grammar");
    let g = load(&xml).unwrap_or_else(|e| panic!("failed to load {xml_name}: {e}"));

    assert_eq!(
        g.syn_features.features.len(),
        expect_syn_features,
        "{xml_name}: syn features"
    );
    assert_eq!(pos_symbol_count(&g), expect_pos, "{xml_name}: POS symbols");
    assert_eq!(
        g.natural_classes.len(),
        expect_nat_classes,
        "{xml_name}: natural classes"
    );
    assert_eq!(
        g.prules.len(),
        expect_prules,
        "{xml_name}: phonological rules"
    );
    assert_eq!(
        g.mrules.len(),
        expect_mrules,
        "{xml_name}: morphological rules"
    );
    assert_eq!(g.templates.len(), expect_templates, "{xml_name}: templates");
    assert_eq!(
        g.entries.len(),
        expect_entries,
        "{xml_name}: lexical entries"
    );
    assert_eq!(g.strata.len(), expect_strata, "{xml_name}: strata");

    // Every morphological AffixProcess rule and every lexical entry is a morpheme.
    let affix_mrules = g
        .mrules
        .iter()
        .filter(|m| matches!(m, MorphRuleDef::AffixProcess(_)))
        .count();
    assert_eq!(
        g.morphemes.len(),
        affix_mrules + g.entries.len(),
        "{xml_name}: morpheme registry = affix-process rules + lexical entries"
    );

    // The empty FS is always interned first (FsId 0).
    assert!(
        !g.fs_interner.is_empty(),
        "{xml_name}: empty FS must be interned"
    );
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn loads_indonesian() {
    // Counts independently confirmed via grep -c on indonesian-hc.xml: 2 syn features, 6 POS symbols, 14 natural classes, 5 prules, 15 mrules, 0 templates, 66 entries, 3 strata.
    check("indonesian-hc.xml", 2, 6, 14, 5, 15, 0, 66, 3);
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn loads_amharic() {
    // Counts independently confirmed via grep -c on amharic-hc.xml: 18 syn features, 16 POS symbols, 17 natural classes, 7 prules, 88 mrules, 15 templates, 76 entries, 3 strata.
    check("amharic-hc.xml", 18, 16, 17, 7, 88, 15, 76, 3);
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn loads_sena() {
    // Counts independently confirmed via grep -c on sena-hc.xml: 5 syn features, 37 POS symbols, 13 natural classes, 0 prules, 140 mrules, 24 templates, 1371 entries, 3 strata.
    check("sena-hc.xml", 5, 37, 13, 0, 140, 24, 1371, 3);
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/{indonesian,amharic,sena}-hc.xml); run with --include-ignored"]
fn dump_grammar_is_deterministic() {
    for name in ["indonesian-hc.xml", "amharic-hc.xml", "sena-hc.xml"] {
        let Some(path) = sample_path(name) else {
            eprintln!("skipping {name}: not present");
            continue;
        };
        let xml = std::fs::read_to_string(&path).unwrap();
        let d1 = load(&xml).unwrap().dump_grammar();
        let d2 = load(&xml).unwrap().dump_grammar();
        assert_eq!(d1, d2, "{name}: dump must be deterministic across re-loads");
    }
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn dump_grammar_reports_headline_counts() {
    let Some(path) = sample_path("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian not present");
        return;
    };
    let xml = std::fs::read_to_string(&path).unwrap();
    let dump = load(&xml).unwrap().dump_grammar();
    assert!(dump.contains("strata=3"), "dump:\n{dump}");
    assert!(dump.contains("mrules=15"));
    assert!(dump.contains("prules=5"));
    assert!(dump.contains("entries=66"));
}

/// A hand-built minimal grammar exercises the loader end-to-end without the corpus files, so it runs in CI too.
#[test]
fn loads_hand_built_minimal_grammar() {
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>Mini</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <HeadFeatures />
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mprA">Alpha</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeature id="mprB">Alpha</MorphologicalPhonologicalRuleFeature>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="cA" /><Segment segment="cB" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mr2 bogus mr1">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>-b</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub1">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput MPRFeatures="mprA">
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>+b</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mr2" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>-a</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub2">
                <MorphologicalInput>
                  <PhoneticSequence id="stem2">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem2" />
                  <InsertSegments><PhoneticShape>+a</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>do</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    let g = load(XML).unwrap();
    assert_eq!(g.name.as_deref(), Some("Mini"));
    assert_eq!(g.syn_features.features.len(), 2); // POS + head (empty HeadFeatures still added)
    assert_eq!(pos_symbol_count(&g), 2);
    assert_eq!(g.mpr_names, vec!["Alpha".to_string(); 2]);
    assert_eq!(g.mpr_names.len(), g.mpr_features.len());
    for (id, expected_xml_id) in [(MprId(0), "mprA"), (MprId(1), "mprB")] {
        let feature = g.mpr_feature(id).expect("loaded MPR id must resolve");
        assert_eq!(feature.xml_id, expected_xml_id);
        assert_eq!(feature.name, g.mpr_names[id.0 as usize]);
    }
    assert_eq!(g.entries[0].authored_id, "e1");
    let pos = &g.syn_features.features[g.syn_features.pos.0 as usize];
    assert_eq!(pos.xml_id, "__pos__");
    match &pos.kind {
        SynFeatureKind::Symbolic { symbols, .. } => {
            assert!(symbols.iter().any(|(id, _)| id == "posV"));
        }
        SynFeatureKind::Complex => panic!("POS must be symbolic"),
    }
    assert_eq!(g.natural_classes.len(), 1);
    assert_eq!(g.strata.len(), 1);
    assert_eq!(g.mrules.len(), 2);
    assert_eq!(g.entries.len(), 1);
    assert_eq!(g.morphemes.len(), 3); // two affix rules + one entry
    assert_eq!(g.allomorph_owners.len(), 3);
    assert_eq!(g.strata[0].mrule_order, MorphRuleOrder::Unordered);
    // Parity-critical: stratum rules follow the morphologicalRules id-list order, not document order, and unknown ids ("bogus") are silently skipped.
    assert_eq!(g.strata[0].mrules, vec![MRuleId(1), MRuleId(0)]);
    // Affix rule references its output MPR feature and has one allomorph.
    let MorphRuleDef::AffixProcess(a) = &g.mrules[0] else {
        panic!("expected affix process rule");
    };
    assert_eq!(a.allomorphs.len(), 1);
    assert!(a.allomorphs[0].out_mpr.contains(MprId(0)));
    // The RHS copies the captured input part then inserts "+b".
    assert!(matches!(
        a.allomorphs[0].rhs[0],
        OutputAction::Copy(PartRef::Input(0))
    ));

    let mut facts_grammar = g;
    if let MorphRuleDef::AffixProcess(def) = &mut facts_grammar.mrules[0] {
        def.partial_reason = Some(PartialMorphemeReason::InflectionalAffixWithoutTemplateSlot);
    }
    facts_grammar.morphemes[0].stratum = StratumId(1);
    let table = facts_grammar.strata[0].table;
    facts_grammar.strata[0].mrules.clear();
    facts_grammar.strata.extend((1..3).map(|i| StratumDef {
        name: Some(format!("S{i}")),
        table,
        mrule_order: MorphRuleOrder::Unordered,
        prules: Vec::new(),
        mrules: Vec::new(),
        templates: Vec::new(),
        entries: Vec::new(),
    }));
    facts_grammar.templates.push(AffixTemplateDef {
        name: Some("template-only".into()),
        is_final: true,
        required_syn_fs: pg_featstruct::FsId(0),
        slots: vec![SlotDef {
            name: Some("slot".into()),
            optional: false,
            zone: TemplateSlotZone::LegacyUnspecified,
            rules: vec![MRuleId(0)],
        }],
    });
    facts_grammar.strata[1].templates.push(TemplateId(0));
    let facts = facts_grammar.final_template_prune_facts().unwrap();
    assert_eq!(facts.partial_rule_count(), 1);
    assert_eq!(facts.partial_rule_at_or_below(), &[false, true, true]);
    assert_eq!(facts.all_templates_final(), &[false, true, false]);
    assert!(facts.slot_rules_disjoint_from_mrules());
    assert_eq!(facts.default_prune_enabled(), &[true, false, false]);
    assert_eq!(facts.disabled_strata(), &[StratumId(1), StratumId(2)]);

    let mut entry_only = load(XML).unwrap();
    entry_only.entries[0].partial_reason = Some(PartialMorphemeReason::StemWithoutCategory);
    let entry_only_facts = entry_only.final_template_prune_facts().unwrap();
    assert_eq!(entry_only_facts.partial_rule_count(), 0);
    assert_eq!(entry_only_facts.partial_rule_at_or_below(), &[false]);
    assert_eq!(entry_only_facts.default_prune_enabled(), &[true]);
    assert!(entry_only_facts.disabled_strata().is_empty());

    let mut mismatch = load(XML).unwrap();
    let table = mismatch.strata[0].table;
    mismatch.strata.push(StratumDef {
        name: Some("Other".into()),
        table,
        mrule_order: MorphRuleOrder::Linear,
        prules: Vec::new(),
        mrules: vec![MRuleId(0)],
        templates: Vec::new(),
        entries: Vec::new(),
    });
    mismatch.strata[0].mrules.clear();
    let err = mismatch
        .final_template_prune_facts()
        .expect_err("ordinary mrule owner mismatch must be rejected");
    assert!(err.to_string().contains("ordinary mrule 0"));
}

#[test]
fn final_template_facts_disable_default_pruning_for_template_overlap() {
    const XML: &str = r#"<HermitCrabInput><Language>
          <Name>Overlap</Name>
          <PartsOfSpeech><PartOfSpeech id="p"><Name>n</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions>
            <SegmentDefinition id="c"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
          </SegmentDefinitions></CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="nc"><Name>C</Name><Segment segment="c" /></SegmentNaturalClass></NaturalClasses>
          <Strata><Stratum characterDefinitionTable="t" morphologicalRules="mr">
            <Name>S</Name><MorphologicalRuleDefinitions><MorphologicalRule id="mr" requiredPartsOfSpeech="p" outputPartOfSpeech="p"><Name>mr</Name>
              <MorphologicalSubrules><MorphologicalSubrule><MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="nc" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="stem" /></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules>
            </MorphologicalRule></MorphologicalRuleDefinitions>
            <AffixTemplates><AffixTemplate id="tpl" final="true"><Name>Tpl</Name><Slot morphologicalRules="mr" /></AffixTemplate></AffixTemplates>
          </Stratum></Strata>
        </Language></HermitCrabInput>"#;
    let grammar = load(XML).expect("overlap remains representable when pruning is disabled");
    let facts = grammar.final_template_prune_facts().unwrap();
    assert!(!facts.slot_rules_disjoint_from_mrules());
    assert_eq!(facts.default_prune_enabled(), &[false]);
    assert_eq!(facts.disabled_strata(), &[StratumId(0)]);
}

/// One partial lexical entry plus one partial affix-process rule, alongside a plain entry and rule of each kind, proves the two counters never leak into each other.
#[test]
fn partial_morpheme_facts_counts_entries_and_rules_independently() {
    fn grammar(entry_partial: bool, rule_partial: bool) -> Grammar {
        let xml = format!(
            r#"<HermitCrabInput><Language>
              <Name>PartialFacts</Name>
              <PartsOfSpeech><PartOfSpeech id="p"><Name>n</Name></PartOfSpeech></PartsOfSpeech>
              <CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions>
                <SegmentDefinition id="c"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
              </SegmentDefinitions></CharacterDefinitionTable>
              <NaturalClasses><SegmentNaturalClass id="nc"><Name>C</Name><Segment segment="c" /></SegmentNaturalClass></NaturalClasses>
              <Strata><Stratum characterDefinitionTable="t" morphologicalRules="rule-plain rule-partial">
                <Name>S</Name>
                <MorphologicalRuleDefinitions>
                  <MorphologicalRule id="rule-plain" requiredPartsOfSpeech="p" outputPartOfSpeech="p"><Name>plain</Name>
                    <MorphologicalSubrules><MorphologicalSubrule><MorphologicalInput><PhoneticSequence id="stem1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="nc" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="stem1" /></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules>
                  </MorphologicalRule>
                  <MorphologicalRule id="rule-partial" requiredPartsOfSpeech="p" outputPartOfSpeech="p" partial="{rule_partial}"><Name>partial</Name>
                    <MorphologicalSubrules><MorphologicalSubrule><MorphologicalInput><PhoneticSequence id="stem2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="nc" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="stem2" /></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules>
                  </MorphologicalRule>
                </MorphologicalRuleDefinitions>
                <LexicalEntries>
                  <LexicalEntry id="entry-plain" partOfSpeech="p"><Allomorphs><Allomorph id="a1"><PhoneticShape>c</PhoneticShape></Allomorph></Allomorphs><Gloss>plain</Gloss></LexicalEntry>
                  <LexicalEntry id="entry-partial" partOfSpeech="p" partial="{entry_partial}"><Allomorphs><Allomorph id="a2"><PhoneticShape>c</PhoneticShape></Allomorph></Allomorphs><Gloss>partial</Gloss></LexicalEntry>
                </LexicalEntries>
              </Stratum></Strata>
            </Language></HermitCrabInput>"#
        );
        load(&xml).expect("valid partial-facts fixture")
    }

    let both = grammar(true, true);
    let facts = both
        .partial_morpheme_facts()
        .expect("valid partial inventory");
    assert_eq!(facts.partial_entry_count(), 1);
    assert_eq!(facts.partial_rule_count(), 1);
    assert_eq!(facts.total_count(), 2);
    assert!(facts.has_partials());
    assert!(facts.authored_ids().any(|id| id == "entry-partial"));
    assert!(facts.authored_ids().any(|id| id == "rule-partial"));
    let entry_reason = facts.identities().find_map(|identity| match identity {
        crate::model::PartialMorphemeIdentity::LexicalEntry { reason, .. } => Some(*reason),
        crate::model::PartialMorphemeIdentity::MorphologicalRule { .. } => None,
    });
    let rule_reason = facts.identities().find_map(|identity| match identity {
        crate::model::PartialMorphemeIdentity::MorphologicalRule { reason, .. } => Some(*reason),
        crate::model::PartialMorphemeIdentity::LexicalEntry { .. } => None,
    });
    assert_eq!(format!("{entry_reason:?}"), "Some(StemWithoutCategory)");
    assert_eq!(format!("{rule_reason:?}"), "Some(Unspecified)");

    let control = grammar(false, false);
    let control_facts = control
        .partial_morpheme_facts()
        .expect("valid partial inventory");
    assert_eq!(control_facts.total_count(), 0);
    assert!(!control_facts.has_partials());

    let entry_only = grammar(true, false);
    let entry_only_facts = entry_only
        .partial_morpheme_facts()
        .expect("valid partial inventory");
    assert_eq!(entry_only_facts.partial_entry_count(), 1);
    assert_eq!(entry_only_facts.partial_rule_count(), 0);

    let rule_only = grammar(false, true);
    let rule_only_facts = rule_only
        .partial_morpheme_facts()
        .expect("valid partial inventory");
    assert_eq!(rule_only_facts.partial_entry_count(), 0);
    assert_eq!(rule_only_facts.partial_rule_count(), 1);
}

/// A morpheme-owned stratum outside the grammar's declared strata must fail loudly, not silently drop the owning rule from the partial inventory.
#[test]
fn partial_morpheme_facts_rejects_invalid_owner_stratum() {
    const XML: &str = r#"<HermitCrabInput><Language>
          <Name>InvalidStratum</Name>
          <PartsOfSpeech><PartOfSpeech id="p"><Name>n</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions>
            <SegmentDefinition id="c"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
          </SegmentDefinitions></CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="nc"><Name>C</Name><Segment segment="c" /></SegmentNaturalClass></NaturalClasses>
          <Strata><Stratum characterDefinitionTable="t" morphologicalRules="rule-partial">
            <Name>S</Name>
            <MorphologicalRuleDefinitions>
              <MorphologicalRule id="rule-partial" requiredPartsOfSpeech="p" outputPartOfSpeech="p" partial="true"><Name>partial</Name>
                <MorphologicalSubrules><MorphologicalSubrule><MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="nc" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="stem" /></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules>
              </MorphologicalRule>
            </MorphologicalRuleDefinitions>
          </Stratum></Strata>
        </Language></HermitCrabInput>"#;
    let mut grammar = load(XML).expect("valid grammar before corruption");
    let MorphRuleDef::AffixProcess(def) = &grammar.mrules[0] else {
        panic!("expected affix process rule");
    };
    let rule_morpheme = def.morpheme;
    grammar.morphemes[rule_morpheme.0 as usize].stratum = StratumId(255);
    let error = grammar
        .partial_morpheme_facts()
        .expect_err("invalid owner stratum must fail");
    assert!(error.to_string().contains("stratum 255"));
}

/// A root-allomorph `<PhoneticShape>` whose text doesn't literally match a character definition must fall back to the `[NatClass]` pattern language instead of erroring the whole allomorph out; a regression here drops not just the allomorph but the whole entry, since this fixture's entry has only that one allomorph.
#[test]
fn root_allomorph_shape_falls_back_to_pattern_language_natural_class_reference() {
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>N3Test</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cE"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncVowel"><Name>Vowel</Name><Segment segment="cA" /><Segment segment="cE" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="n">
            <Allomorphs>
              <Allomorph id="a1"><PhoneticShape>b[Vowel]t</PhoneticShape></Allomorph>
            </Allomorphs>
            <Gloss>bVt</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    let g = load(XML).unwrap_or_else(|e| panic!("grammar failed to load: {e}"));
    assert_eq!(g.entries.len(), 1, "the lexical entry itself must survive");
    let entry = &g.entries[0];
    assert_eq!(
        entry.allomorphs.len(),
        1,
        "the allomorph must NOT be silently dropped (pattern-language fallback missing?)"
    );
    let shape = &entry.allomorphs[0].shape.shape;
    let interior: Vec<_> = shape.interior().collect();
    assert_eq!(interior.len(), 3, "b, [Vowel], t");
    let a_id = g.char_tables[0].lookup_nfd("a").unwrap();
    let e_id = g.char_tables[0].lookup_nfd("e").unwrap();
    assert_eq!(interior[0].2, g.char_tables[0].lookup_nfd("b").unwrap().0);
    assert_eq!(interior[2].2, g.char_tables[0].lookup_nfd("t").unwrap().0);
    assert_eq!(
        interior[1].2,
        pg_shape::NO_CHAR_DEF,
        "the class reference is an abstract node"
    );
    match shape.node_cd_set(interior[1].0) {
        pg_shape::EffectiveCdSet::Members(b) => {
            assert!(b.contains(a_id.0) && b.contains(e_id.0));
            assert_eq!(b.count(), 2);
        }
        other => panic!("expected Members({{a,e}}), got {other:?}"),
    }
}

/// `RootAllomorphDef.is_pattern` must match C#'s `RootAllomorph` ctor rule exactly: any interior node that is iterative, or optional-and-not-a-boundary, classifies the whole allomorph as a lexical pattern.
#[test]
fn is_pattern_matches_the_csharp_root_allomorph_classification_rule() {
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PatternClassTest</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cE"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncVowel"><Name>Vowel</Name><Segment segment="cA" /><Segment segment="cE" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name><Segment segment="cB" /><Segment segment="cP" /><Segment segment="cI" /><Segment segment="cT" /><Segment segment="cA" /><Segment segment="cE" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e_star" partOfSpeech="n">
            <Allomorphs><Allomorph id="a_star"><PhoneticShape>[Any]*</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>star</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e_opt" partOfSpeech="n">
            <Allomorphs><Allomorph id="a_opt"><PhoneticShape>([Vowel])</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>opt</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e_mandatory_class" partOfSpeech="n">
            <Allomorphs><Allomorph id="a_mandatory_class"><PhoneticShape>b[Vowel]t</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>bVt</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e_plain" partOfSpeech="n">
            <Allomorphs><Allomorph id="a_plain"><PhoneticShape>pit</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pit</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e_boundary" partOfSpeech="n">
            <Allomorphs><Allomorph id="a_boundary"><PhoneticShape>pi+t</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>piPlusT</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    let g = load(XML).unwrap_or_else(|e| panic!("grammar failed to load: {e}"));
    assert_eq!(
        g.entries.len(),
        5,
        "every entry must survive with its one allomorph"
    );
    let is_pattern = |i: usize| {
        let e = &g.entries[i];
        assert_eq!(e.allomorphs.len(), 1, "entry {i}: exactly one allomorph");
        e.allomorphs[0].is_pattern
    };
    assert!(is_pattern(0), "[Any]* : iterative -> pattern");
    assert!(
        is_pattern(1),
        "([Vowel]) : optional, not a boundary -> pattern"
    );
    assert!(
        !is_pattern(2),
        "b[Vowel]t : mandatory (non-optional, non-iterative) class -> NOT a pattern"
    );
    assert!(!is_pattern(3), "pit : plain literal shape -> NOT a pattern");
    assert!(
        !is_pattern(4),
        "pi+t : the only optional node is the boundary ('+' is always Optional after \
         segmentation) -> NOT a pattern (the kind != Boundary guard)"
    );
}

/// `<FootFeatures>` is not hard-linted unsupported: it loads, adds a foot complex feature mirroring `<HeadFeatures>`/`syn.head` exactly, and its declared features join the shared syntactic feature namespace.
#[test]
fn foot_features_loads_as_a_complex_feature_mirroring_head() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="p"><Name>n</Name></PartOfSpeech></PartsOfSpeech>
          <FootFeatures><SymbolicFeature id="f"><Name>x</Name><Symbols><Symbol id="s">+</Symbol></Symbols></SymbolicFeature></FootFeatures>
        </Language></HermitCrabInput>"#;
    let g = load(XML).expect("FootFeatures must load, not lint unsupported");
    assert!(
        g.syn_features.foot.is_some(),
        "foot complex feature must be present"
    );
    assert!(
        g.syn_features.feature_by_xml_id("f").is_some(),
        "foot-declared feature 'f' must join the syntactic feature namespace"
    );
}

/// An absent `<FootFeatures>` element must still leave `syn.foot == None`, the exact `<HeadFeatures>`-absent behavior already pinned elsewhere.
#[test]
fn absent_foot_features_element_leaves_foot_none() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="p"><Name>n</Name></PartOfSpeech></PartsOfSpeech>
        </Language></HermitCrabInput>"#;
    let g = load(XML).expect("grammar with no FootFeatures must still load");
    assert!(g.syn_features.foot.is_none());
}

/// Regression test for the `<AffixTemplate final>` default-value bug, generalized into a full sweep of every DTD `ATTLIST` default reachable from `load.rs`.
/// See docs/research/pg-grammar-dtd-attribute-defaults.md for the full pinned list and the two attributes deliberately out of scope.
#[test]
fn dtd_attribute_defaults_match_spec() {
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>Defaults</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <HeadFeatures />
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mprA">Alpha</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeatureGroup features="mprA"><Name>G</Name></MorphologicalPhonologicalRuleFeatureGroup>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="cA" /></SegmentNaturalClass>
    </NaturalClasses>
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
      <Stratum characterDefinitionTable="t1" phonologicalRules="pr1">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>-a</Name>
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
        <AffixTemplates>
          <AffixTemplate>
            <Name>T1</Name>
            <Slot morphologicalRules="mr1"><Name>Sl1</Name></Slot>
          </AffixTemplate>
          <AffixTemplate final="false">
            <Name>T2</Name>
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
    let g = load(XML).unwrap();

    // AffixTemplate final: omitted -> true (the bug), explicit "false" -> false.
    assert!(
        g.templates[0].is_final,
        "AffixTemplate final defaults to true per DTD"
    );
    assert!(
        !g.templates[1].is_final,
        "explicit final=\"false\" must still be honored"
    );

    // Slot optional: omitted -> false.
    assert!(!g.templates[0].slots[0].optional);
    assert_eq!(
        g.templates[0].slots[0].zone,
        crate::model::TemplateSlotZone::LegacyUnspecified,
        "HC XML has no physical prefix/suffix slot metadata"
    );

    // MorphologicalRule blockable/partial/multipleApplication: all omitted.
    let MorphRuleDef::AffixProcess(mr1) = &g.mrules[0] else {
        panic!("expected affix process rule");
    };
    assert!(
        mr1.blockable,
        "MorphologicalRule blockable defaults to true per DTD"
    );
    assert!(
        !mr1.is_partial(),
        "MorphologicalRule partial defaults to false per DTD"
    );
    assert_eq!(
        mr1.max_apps, 1,
        "MorphologicalRule multipleApplication defaults to 1 per DTD"
    );

    // MorphologicalOutput redupMorphType: omitted -> Implicit.
    assert_eq!(mr1.allomorphs[0].redup_hint, ReduplicationHint::Implicit);

    // Allomorph isBound: omitted -> false.
    assert!(
        !g.entries[0].allomorphs[0].is_bound,
        "Allomorph isBound defaults to false per DTD"
    );

    // MorphologicalPhonologicalRuleFeatureGroup matchType/outputType: both omitted.
    assert_eq!(g.mpr_groups[0].match_type, MprGroupMatchType::Any);
    assert_eq!(g.mpr_groups[0].output, MprGroupOutput::Overwrite);

    // Stratum morphologicalRuleOrder: omitted -> Linear.
    assert_eq!(g.strata[0].mrule_order, MorphRuleOrder::Linear);

    // PhonologicalRule multipleApplicationOrder: omitted -> leftToRightIterative.
    let PhonRuleDef::Rewrite(pr0) = &g.prules[0] else {
        panic!("expected a rewrite rule");
    };
    assert_eq!(pr0.mode, RewriteMode::Iterative);
    assert_eq!(pr0.dir, Dir::LeftToRight);
}

/// The former stopgap lint (`RewriteMode::Simultaneous` hard-failed at load) is gone now that Simultaneous has real execution semantics: a `multipleApplicationOrder="simultaneous"` rule loads successfully and round-trips into `RewriteRuleDef.mode`.
#[test]
fn rewrite_mode_simultaneous_loads_and_round_trips() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1" multipleApplicationOrder="simultaneous">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
        </Language></HermitCrabInput>"#;
    let g = load(XML).unwrap_or_else(|e| panic!("simultaneous rule must load: {e}"));
    let PhonRuleDef::Rewrite(pr0) = &g.prules[0] else {
        panic!("expected a rewrite rule");
    };
    assert_eq!(
        pr0.mode,
        RewriteMode::Simultaneous,
        "multipleApplicationOrder=\"simultaneous\" must round-trip"
    );

    // Sanity: the same rule with the default order must still load fine and round-trip Iterative, confirming this isn't a blanket "always Simultaneous now" bug.
    let iterative_xml = XML.replace(r#" multipleApplicationOrder="simultaneous""#, "");
    let g2 = load(&iterative_xml).expect("an ordinary iterative rule must still load");
    let PhonRuleDef::Rewrite(pr0b) = &g2.prules[0] else {
        panic!("expected a rewrite rule");
    };
    assert_eq!(pr0b.mode, RewriteMode::Iterative);
}

/// Pins `RewriteSubruleDef::self_opaquing`'s per-kind formula directly across five rule shapes, mirroring C#'s own dispatch rather than just eyeballing agreement.
/// See docs/research/pg-grammar-self-opaquing-pin-cases.md for what each of the five cases (prA-prE) pins and why.
#[test]
fn self_opaquing_pin_semantics_match_node_pins() {
    const XML: &str = r#"<HermitCrabInput><Language><Name>SelfOpaquingProbe</Name>
          <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
          <PhonologicalFeatureSystem>
            <SymbolicFeature id="featCons"><Name>cons</Name>
              <Symbols><Symbol id="symConsP">+</Symbol><Symbol id="symConsM">-</Symbol></Symbols>
            </SymbolicFeature>
            <SymbolicFeature id="featVoi"><Name>voi</Name>
              <Symbols><Symbol id="symVoiP">+</Symbol><Symbol id="symVoiM">-</Symbol></Symbols>
            </SymbolicFeature>
          </PhonologicalFeatureSystem>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations>
                <FeatureValue feature="featCons" symbolValues="symConsP" />
                <FeatureValue feature="featVoi" symbolValues="symVoiM" />
              </SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <FeatureNaturalClass id="ncStop"><Name>Stop</Name>
              <FeatureValue feature="featCons" symbolValues="symConsP" />
            </FeatureNaturalClass>
            <FeatureNaturalClass id="ncVoiced"><Name>Voiced</Name>
              <FeatureValue feature="featVoi" symbolValues="symVoiP" />
            </FeatureNaturalClass>
            <FeatureNaturalClass id="ncVoiceless"><Name>Voiceless</Name>
              <FeatureValue feature="featVoi" symbolValues="symVoiM" />
            </FeatureNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="prA" multipleApplicationOrder="simultaneous"><Name>ruleA</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules><PhonologicalSubrule>
                <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
                <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
              </PhonologicalSubrule></PhonologicalSubrules>
            </PhonologicalRule>
            <PhonologicalRule id="prB" multipleApplicationOrder="simultaneous"><Name>ruleB</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules><PhonologicalSubrule>
                <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
                <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncVoiceless" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
              </PhonologicalSubrule></PhonologicalSubrules>
            </PhonologicalRule>
            <PhonologicalRule id="prC"><Name>ruleC</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules><PhonologicalSubrule>
                <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
                <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncVoiceless" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
              </PhonologicalSubrule></PhonologicalSubrules>
            </PhonologicalRule>
            <PhonologicalRule id="prD" multipleApplicationOrder="simultaneous"><Name>ruleD</Name>
              <PhonologicalSubrules><PhonologicalSubrule>
                <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
              </PhonologicalSubrule></PhonologicalSubrules>
            </PhonologicalRule>
            <PhonologicalRule id="prE" multipleApplicationOrder="simultaneous"><Name>ruleE</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules><PhonologicalSubrule>
                <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
              </PhonologicalSubrule></PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
        </Language></HermitCrabInput>"#;
    let g = load(XML).unwrap_or_else(|e| panic!("self-opaquing probe grammar must load: {e}"));
    let rewrite = |i: usize| -> &RewriteRuleDef {
        let PhonRuleDef::Rewrite(r) = &g.prules[i] else {
            panic!("expected a rewrite rule at {i}")
        };
        r
    };
    assert!(
        !rewrite(0).subrules[0].self_opaquing,
        "prA: RHS pin unifiable with its environment -> Normal reapply, not self-opaquing"
    );
    assert!(
        rewrite(1).subrules[0].self_opaquing,
        "prB: RHS pin NOT unifiable with its environment (disjoint voi bits) -> self-opaquing"
    );
    assert!(
        !rewrite(2).subrules[0].self_opaquing,
        "prC: same patterns as prB but Iterative mode -> mode gate short-circuits to false"
    );
    assert!(
        rewrite(3).subrules[0].self_opaquing,
        "prD: Epenthesis + Simultaneous is unconditionally self-opaquing"
    );
    assert!(
        !rewrite(4).subrules[0].self_opaquing,
        "prE: Narrow/Expansion is irrelevant/always false regardless of rule.mode"
    );
}

/// Pins the exact `>= 64` vs `> 64` boundary for the two symbol-count caps a real grammar is most likely to hit; neither edge was previously exercised by a unit test.
fn pos_grammar_with_n_symbols(n: usize) -> String {
    let mut pos = String::new();
    for i in 0..n {
        pos.push_str(&format!(
            r#"<PartOfSpeech id="p{i}"><Name>p{i}</Name></PartOfSpeech>"#
        ));
    }
    format!(
        r#"<HermitCrabInput><Language><Name>X</Name><PartsOfSpeech>{pos}</PartsOfSpeech></Language></HermitCrabInput>"#
    )
}

#[test]
fn pos_symbol_cap_63_ok_64_rejected() {
    let ok = load(&pos_grammar_with_n_symbols(63));
    assert!(
        ok.is_ok(),
        "63 parts of speech must load fine: {:?}",
        ok.err()
    );
    let bad = load(&pos_grammar_with_n_symbols(64));
    assert!(
        matches!(bad, Err(GrammarError::Unsupported(_))),
        "64 parts of speech must be rejected at the 64-symbol boundary, got {bad:?}"
    );
}

fn phon_feature_grammar_with_n_symbols(n: usize) -> String {
    let mut syms = String::new();
    for i in 0..n {
        syms.push_str(&format!(r#"<Symbol id="s{i}">v{i}</Symbol>"#));
    }
    format!(
        r#"<HermitCrabInput><Language><Name>X</Name>
              <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
              <PhonologicalFeatureSystem>
                <SymbolicFeature id="f1"><Name>f</Name><Symbols>{syms}</Symbols></SymbolicFeature>
              </PhonologicalFeatureSystem>
            </Language></HermitCrabInput>"#
    )
}

#[test]
fn phonological_symbolic_feature_cap_63_ok_64_rejected() {
    let ok = load(&phon_feature_grammar_with_n_symbols(63));
    assert!(
        ok.is_ok(),
        "a 63-symbol phonological feature must load fine: {:?}",
        ok.err()
    );
    let bad = load(&phon_feature_grammar_with_n_symbols(64));
    assert!(
        matches!(bad, Err(GrammarError::Unsupported(_))),
        "a 64-symbol phonological feature must be rejected at the 64-symbol boundary, got {bad:?}"
    );
}

// --- Well-formedness / DTD-required-element strictness ------------------------------------

const WELL_FORMED_MINIMAL_XML: &str = r#"<HermitCrabInput><Language><Name>WellFormed</Name>
      <!-- a comment with a single hyphen - not a double one -->
      <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
    </Language></HermitCrabInput>"#;

#[test]
fn well_formed_document_with_a_hyphen_in_a_comment_still_loads() {
    load(WELL_FORMED_MINIMAL_XML).unwrap_or_else(|e| panic!("must load: {e}"));
}

#[test]
fn double_hyphen_inside_a_comment_is_refused_not_silently_tolerated() {
    let xml = r#"<HermitCrabInput><Language><Name>X</Name>
          <!-- this comment uses -- an illegal double hyphen -->
          <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
        </Language></HermitCrabInput>"#;
    let err = load(xml).expect_err("a `--` inside a comment must be refused, not tolerated");
    let msg = err.to_string();
    assert!(
        msg.contains("comment"),
        "the refusal must name the problem (a comment), got: {msg}"
    );
}

#[test]
fn missing_parts_of_speech_is_refused_not_silently_zero_pos() {
    let xml = r#"<HermitCrabInput><Language><Name>NoPos</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
        </Language></HermitCrabInput>"#;
    let err = load(xml).expect_err(
        "HermitCrabInput.dtd requires <PartsOfSpeech> (at least one <PartOfSpeech>); a document without it must be refused",
    );
    let msg = err.to_string();
    assert!(
        msg.contains("PartsOfSpeech"),
        "the refusal must name the missing element, got: {msg}"
    );
}

#[test]
fn empty_parts_of_speech_block_is_also_refused() {
    let xml = r#"<HermitCrabInput><Language><Name>EmptyPos</Name>
          <PartsOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
        </Language></HermitCrabInput>"#;
    let err = load(xml)
        .expect_err("<PartsOfSpeech> with zero <PartOfSpeech> children violates PartOfSpeech+ and must be refused");
    assert!(matches!(err, GrammarError::Xml(_)));
}
