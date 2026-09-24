use super::*;

fn duplicate_stem(snapshot: &mut Snapshot) {
    let entry = &mut snapshot.lexicon.entries[0];
    let mut duplicate = entry.msas[0].clone();
    if let Msa::Stem { guid, .. } = &mut duplicate {
        *guid = "msa-duplicate".into();
    }
    entry.msas.push(duplicate);
    entry.senses.push(Sense {
        guid: "sense-duplicate".into(),
        gloss: vec![ws("en", "cat")],
        definition: vec![],
        msa: Some("msa-duplicate".into()),
    });
}

fn add_variant(snapshot: &mut Snapshot, component: &str) {
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-variant".into(),
        citation_form: vec![],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![simple_allomorph("allo-variant", MorphType::Stem, "kumi")],
        msas: vec![],
        senses: vec![],
        entry_refs: vec![EntryRef::Variant {
            guid: "entryref-variant".into(),
            component_lexemes: vec![component.into()],
            variant_entry_types: vec![],
        }],
    });
}

#[test]
fn unique_stem_duplicate_keeps_first_guid_gloss_and_inventory() {
    let (mut snapshot, f) = fixture();
    duplicate_stem(&mut snapshot);
    let (grammar, warnings, inventory, _) = compile_recording_ok(&snapshot);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(grammar.entries.len(), 1);
    let morpheme = &grammar.morphemes[grammar.entries[0].morpheme.0 as usize];
    assert_eq!(
        morpheme.source_msa_guid.as_deref(),
        Some(f.stem_msa.as_str())
    );
    assert_eq!(morpheme.gloss.as_deref(), Some("dog"));
    let duplicate = InventoryKey::object(InventoryKind::Msa, "msa-duplicate");
    assert!(inventory.considered.contains(&duplicate));
    assert!(!inventory.selected.contains(&duplicate));
    assert!(!inventory.represented.contains(&duplicate));
}

#[test]
fn unique_stem_distinct_grammatical_msas_survive() {
    let (mut snapshot, _) = fixture();
    duplicate_stem(&mut snapshot);
    if let Msa::Stem { part_of_speech, .. } = &mut snapshot.lexicon.entries[0].msas[1] {
        *part_of_speech = None;
    }
    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(grammar.entries.len(), 2);
}

#[test]
fn unique_stem_same_msa_across_entries_survives() {
    let (mut snapshot, _) = fixture();
    let mut homonym = snapshot.lexicon.entries[0].clone();
    homonym.guid = "entry-homonym".into();
    homonym.allomorphs[0].guid = "allo-homonym".into();
    if let Msa::Stem { guid, .. } = &mut homonym.msas[0] {
        *guid = "msa-homonym".into();
    }
    homonym.senses[0].guid = "sense-homonym".into();
    homonym.senses[0].msa = Some("msa-homonym".into());
    snapshot.lexicon.entries.push(homonym);
    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(grammar.entries.len(), 2);
}

#[test]
fn unique_stem_entry_linked_variant_uses_first_equivalent_msa() {
    let (mut snapshot, f) = fixture();
    duplicate_stem(&mut snapshot);
    add_variant(&mut snapshot, &f.stem_entry);
    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(grammar.entries.len(), 2);
    let variant = grammar
        .entries
        .iter()
        .find(|e| e.authored_id == "entry-variant")
        .unwrap();
    assert_eq!(
        grammar.morphemes[variant.morpheme.0 as usize]
            .source_msa_guid
            .as_deref(),
        Some("msa-stem")
    );
}

#[test]
fn unique_stem_sense_linked_variant_keeps_exact_referenced_msa() {
    let (mut snapshot, _) = fixture();
    duplicate_stem(&mut snapshot);
    add_variant(&mut snapshot, "sense-duplicate");
    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "{warnings:?}");
    let variant = grammar
        .entries
        .iter()
        .find(|e| e.authored_id == "entry-variant")
        .unwrap();
    let morpheme = &grammar.morphemes[variant.morpheme.0 as usize];
    assert_eq!(morpheme.source_msa_guid.as_deref(), Some("msa-duplicate"));
    assert_eq!(morpheme.gloss.as_deref(), Some("cat"));
}

#[test]
fn unique_stem_does_not_deduplicate_affix_msas() {
    let (mut snapshot, _) = fixture();
    let mut duplicate = snapshot.lexicon.entries[1].msas[0].clone();
    if let Msa::Inflectional { guid, .. } = &mut duplicate {
        *guid = "msa-suffix-duplicate".into();
    }
    snapshot.lexicon.entries[1].msas.push(duplicate);
    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        grammar
            .mrules
            .iter()
            .filter(|r| matches!(r, MorphRuleDef::AffixProcess(_)))
            .count(),
        2
    );
}

#[test]
fn unique_stem_missing_duplicate_in_active_prohibition_still_refuses() {
    let (mut snapshot, f) = fixture();
    duplicate_stem(&mut snapshot);
    snapshot
        .morphology
        .adhoc_prohibitions
        .push(AdhocProhibition::Morpheme {
            guid: "prohibition-missing-duplicate".into(),
            disabled: false,
            primary: f.stem_msa,
            others: vec!["msa-duplicate".into()],
            adjacency: Adjacency::Anywhere,
        });
    let err =
        compile_project(&snapshot).expect_err("an active prohibition must not silently disappear");
    assert!(err
        .issues()
        .iter()
        .any(|i| i.code == "grammar.adhoc-prohibition.unresolved" && i.fatal));
}

#[test]
fn unique_stem_absent_inflection_class_differs_from_explicit_default() {
    let (mut snapshot, _) = fixture();
    duplicate_stem(&mut snapshot);
    snapshot.morphology.parts_of_speech[0]
        .inflection_classes
        .push(InflectionClass {
            guid: "class-default".into(),
            name: "Default".into(),
            abbreviation: "def".into(),
            children: vec![],
        });
    snapshot.morphology.parts_of_speech[0].default_inflection_class = Some("class-default".into());
    if let Msa::Stem {
        inflection_class, ..
    } = &mut snapshot.lexicon.entries[0].msas[1]
    {
        *inflection_class = Some("class-default".into());
    }
    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(grammar.entries.len(), 2);
}

#[test]
fn unique_stem_preserves_complete_parse_identity_and_rejection() {
    let (mut snapshot, _) = fixture();
    let (baseline, _) = pg_grammar::compile_project(&snapshot).unwrap();
    duplicate_stem(&mut snapshot);
    let (deduplicated, _) = pg_grammar::compile_project(&snapshot).unwrap();
    for word in ["kumata", "kuma"] {
        let parse = |grammar: &pg_grammar::model::Grammar| {
            let outcome = pg_parse::Morpher::new(grammar, usize::MAX).parse_word(word);
            assert!(
                !outcome.capped && !outcome.timed_out && !outcome.invalid_shape && !outcome.guessed
            );
            let mut identities: Vec<_> = outcome
                .structured
                .iter()
                .map(|a| pg_parse::identity::AnalysisIdentity::project(a, grammar).unwrap())
                .collect();
            identities.sort();
            identities
        };
        let expected = parse(&baseline);
        assert_eq!(
            expected.is_empty(),
            word == "kuma",
            "non-vacuity for {word}"
        );
        assert_eq!(
            parse(&deduplicated),
            expected,
            "identity multiset for {word}"
        );
    }
}

#[test]
fn unique_stem_clitic_lexical_dedup_keeps_both_affix_rules() {
    let (mut snapshot, _) = fixture();
    duplicate_stem(&mut snapshot);
    let entry = &mut snapshot.lexicon.entries[0];
    entry.lexeme_morph_type = MorphType::Enclitic;
    entry.allomorphs[0].morph_type = MorphType::Enclitic;
    let (grammar, warnings) = compile_project(&snapshot).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(grammar.entries.len(), 1);
    let clitic_rules: Vec<_> = grammar
        .mrules
        .iter()
        .filter_map(|rule| match rule {
            MorphRuleDef::AffixProcess(rule) if !rule.is_template_rule => Some(rule),
            _ => None,
        })
        .collect();
    assert_eq!(clitic_rules.len(), 2);
}
