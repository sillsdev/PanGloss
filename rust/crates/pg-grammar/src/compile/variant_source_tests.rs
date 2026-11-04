//! Variant sources that HCLoader accepts but the compiler once skipped: complex-form refs on sense-less entries, and clitic-typed variant forms.

use pg_snapshot::lexicon::{EntryRef, LexEntry};
use pg_snapshot::morphology::MorphType;
use pg_snapshot::Snapshot;

use super::tests::{compile_recording_ok, fixture, simple_allomorph, ws};
use super::{compile_project, compile_project_with, CompileOptions};
use crate::model::{Grammar, StratumId};

fn variant_entry(guid: &str, morph_type: MorphType, form: &str, er: EntryRef) -> LexEntry {
    LexEntry {
        guid: guid.to_string(),
        citation_form: vec![ws("sen", form)],
        lexeme_morph_type: morph_type,
        allomorphs: vec![simple_allomorph(&format!("allo-{guid}"), morph_type, form)],
        msas: Vec::new(),
        senses: Vec::new(),
        entry_refs: vec![er],
    }
}

fn complex_ref(component: &str) -> EntryRef {
    EntryRef::ComplexForm {
        guid: "entryref-complex".to_string(),
        component_lexemes: vec![component.to_string()],
        complex_entry_types: vec!["complex-type".to_string()],
    }
}

fn variant_ref(component: &str) -> EntryRef {
    EntryRef::Variant {
        guid: "entryref-variant".to_string(),
        component_lexemes: vec![component.to_string()],
        variant_entry_types: Vec::new(),
    }
}

fn entry_stratum(grammar: &Grammar, authored: &str) -> Option<StratumId> {
    let entry = grammar.entries.iter().find(|e| e.authored_id == authored)?;
    Some(grammar.morphemes[entry.morpheme.0 as usize].stratum)
}

fn stratum_has_entry(grammar: &Grammar, stratum: usize, authored: &str) -> bool {
    grammar.strata[stratum]
        .entries
        .iter()
        .any(|id| grammar.entries[id.0 as usize].authored_id == authored)
}

#[test]
fn complex_form_ref_on_a_senseless_entry_builds_a_variant_like_hcloader() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(variant_entry(
        "entry-cf",
        MorphType::Stem,
        "kumi",
        complex_ref(&f.stem_entry),
    ));
    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        grammar.entries.len(),
        2,
        "base stem plus the complex-form variant"
    );
    assert_eq!(entry_stratum(&grammar, "entry-cf"), Some(StratumId(0)));
}

#[test]
fn complex_form_ref_builds_the_same_grammar_as_an_untyped_variant_ref() {
    let (mut with_complex, f) = fixture();
    with_complex.lexicon.entries.push(variant_entry(
        "entry-cf",
        MorphType::Stem,
        "kumi",
        complex_ref(&f.stem_entry),
    ));
    let mut with_variant: Snapshot = with_complex.clone();
    with_variant.lexicon.entries.last_mut().unwrap().entry_refs = vec![variant_ref(&f.stem_entry)];
    let a = compile_project(&with_complex).expect("complex").0;
    let b = compile_project(&with_variant).expect("variant").0;
    assert_eq!(a.entries.len(), b.entries.len());
    assert_eq!(a.morphemes.len(), b.morphemes.len());
}

#[test]
fn complex_form_ref_on_an_entry_with_senses_builds_nothing() {
    let (mut snapshot, f) = fixture();
    let mut entry = variant_entry(
        "entry-cf",
        MorphType::Stem,
        "kumi",
        complex_ref(&f.stem_entry),
    );
    entry.senses.push(pg_snapshot::lexicon::Sense {
        guid: "sense-cf".to_string(),
        gloss: Vec::new(),
        definition: Vec::new(),
        msa: None,
    });
    snapshot.lexicon.entries.push(entry);
    let (grammar, _) = compile_project(&snapshot).expect("must compile");
    assert_eq!(
        grammar.entries.len(),
        1,
        "HCLoader reads entry refs only when there are no senses"
    );
}

#[test]
fn complex_form_ref_guid_is_authored_and_represented_when_it_yields_a_variant() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(variant_entry(
        "entry-cf",
        MorphType::Stem,
        "kumi",
        complex_ref(&f.stem_entry),
    ));
    let (_grammar, warnings, inventory, _issues) = compile_recording_ok(&snapshot);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let key = pg_snapshot::InventoryKey::object(
        pg_snapshot::InventoryKind::EntryReference,
        "entryref-complex".to_string(),
    );
    assert!(inventory.authored.contains(&key));
    assert!(inventory.represented.contains(&key));
}

#[test]
fn clitic_typed_variant_of_a_stem_is_a_clitic_stratum_entry() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(variant_entry(
        "entry-clitic-variant",
        MorphType::Enclitic,
        "mi",
        variant_ref(&f.stem_entry),
    ));
    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        entry_stratum(&grammar, "entry-clitic-variant"),
        Some(StratumId(1))
    );
    assert!(stratum_has_entry(&grammar, 1, "entry-clitic-variant"));
    assert!(!stratum_has_entry(&grammar, 0, "entry-clitic-variant"));
}

#[test]
fn clitic_typed_variant_of_a_stem_also_gets_a_clitic_stratum_rule() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(variant_entry(
        "entry-clitic-variant",
        MorphType::Enclitic,
        "mi",
        variant_ref(&f.stem_entry),
    ));
    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let grammar = &output.grammar;
    let rule_ids: Vec<crate::model::MRuleId> = grammar
        .allomorph_sources
        .iter()
        .zip(&grammar.allomorph_owners)
        .filter(|(src, _)| {
            src.form_guids
                .contains(&Some("allo-entry-clitic-variant".to_string()))
        })
        .filter_map(|(_, owner)| match owner {
            crate::model::AllomorphOwner::Affix(rule, _) => Some(*rule),
            _ => None,
        })
        .collect();
    assert!(
        !rule_ids.is_empty(),
        "the variant's clitic form must become an affix-process allomorph"
    );
    let in_stratum = |stratum: usize| {
        rule_ids
            .iter()
            .all(|r| grammar.strata[stratum].mrules.contains(r))
    };
    assert!(in_stratum(1), "clitic stratum must hold the variant's rule");
    assert!(rule_ids
        .iter()
        .all(|r| !grammar.strata[0].mrules.contains(r)));
}

#[test]
fn suffix_typed_variant_of_a_stem_is_a_rule_in_the_morphology_stratum_and_no_stem_entry() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(variant_entry(
        "entry-suffix-variant",
        MorphType::Suffix,
        "mi",
        variant_ref(&f.stem_entry),
    ));
    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let grammar = &output.grammar;
    assert!(
        output.warnings.is_empty(),
        "unexpected warnings: {:?}",
        output.warnings
    );
    assert!(
        grammar
            .entries
            .iter()
            .all(|e| e.authored_id != "entry-suffix-variant"),
        "an affix-only variant has no stem form, so HCLoader builds no lexical entry for it"
    );
    let rule_ids: Vec<crate::model::MRuleId> = grammar
        .allomorph_sources
        .iter()
        .zip(&grammar.allomorph_owners)
        .filter(|(src, _)| {
            src.form_guids
                .contains(&Some("allo-entry-suffix-variant".to_string()))
        })
        .filter_map(|(_, owner)| match owner {
            crate::model::AllomorphOwner::Affix(rule, _) => Some(*rule),
            _ => None,
        })
        .collect();
    assert!(
        !rule_ids.is_empty(),
        "the suffix form becomes an affix-process allomorph under the stem main"
    );
    assert!(rule_ids
        .iter()
        .all(|r| grammar.strata[0].mrules.contains(r) && !grammar.strata[1].mrules.contains(r)));
}
