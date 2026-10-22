//! FieldWorks `<HC><MergeMSAs>` (HCLoader.cs:652-704, 910-923, 2265-2268): equal MSAs of one entry load as their sense-bearing representative at every loader site.

use std::collections::BTreeSet;

use pg_snapshot::lexicon::{Allomorph, EntryRef, EqualMsas, LexEntry, Lexicon, Msa, Sense};
use pg_snapshot::morphology::{AdhocProhibition, Adjacency, MorphType, Morphology, PartOfSpeech};
use pg_snapshot::phonology::{BoundaryMarker, Phoneme, Phonology};
use pg_snapshot::project::Project;
use pg_snapshot::{FeatureSystems, Snapshot, WsForm};

fn ws(form: &str) -> WsForm {
    WsForm {
        ws: "sen".to_string(),
        form: form.to_string(),
    }
}

fn allomorph(guid: &str, morph_type: MorphType, form: &str) -> Allomorph {
    Allomorph {
        guid: guid.to_string(),
        morph_type,
        is_abstract: false,
        forms: vec![ws(form)],
        environments: Vec::new(),
        positions: Vec::new(),
        stem_name: None,
        inflection_classes: Vec::new(),
        ms_env_features: None,
        ms_env_part_of_speech: None,
        process: None,
    }
}

fn stem_msa(guid: &str) -> Msa {
    Msa::Stem {
        guid: guid.to_string(),
        part_of_speech: Some("pos-n".to_string()),
        inflection_class: None,
        features: None,
        exception_features: Vec::new(),
        from_parts_of_speech: Vec::new(),
        slots: Vec::new(),
    }
}

fn affix_msa(guid: &str) -> Msa {
    Msa::Unclassified {
        guid: guid.to_string(),
        part_of_speech: Some("pos-n".to_string()),
    }
}

fn sense(guid: &str, msa: &str) -> Sense {
    Sense {
        guid: guid.to_string(),
        gloss: vec![WsForm {
            ws: "en".to_string(),
            form: guid.to_string(),
        }],
        definition: Vec::new(),
        msa: Some(msa.to_string()),
    }
}

/// An entry with one allomorph, the given MSAs, and a sense for each MSA named in `with_sense`.
fn entry(
    guid: &str,
    morph_type: MorphType,
    form: &str,
    msas: Vec<Msa>,
    with_sense: &[&str],
) -> LexEntry {
    LexEntry {
        guid: guid.to_string(),
        citation_form: vec![ws(form)],
        lexeme_morph_type: morph_type,
        allomorphs: vec![allomorph(&format!("allo-{guid}"), morph_type, form)],
        msas,
        senses: with_sense
            .iter()
            .map(|msa| sense(&format!("sense-{msa}"), msa))
            .collect(),
        entry_refs: Vec::new(),
    }
}

fn variant(guid: &str, morph_type: MorphType, form: &str, component: &str) -> LexEntry {
    LexEntry {
        entry_refs: vec![EntryRef::Variant {
            guid: format!("ref-{guid}"),
            component_lexemes: vec![component.to_string()],
            variant_entry_types: Vec::new(),
        }],
        ..entry(guid, morph_type, form, Vec::new(), &[])
    }
}

/// Equal pairs k1/k2 (only k2 has a sense), p1/p2 (no senses), t1/t2, u1/u2, s1/s2; variants `kot`, `kit` (of t2's sense), `-o` (of `-e`); u2 prohibits s2 with s1.
fn snapshot(merge: bool, extracted: bool) -> Snapshot {
    let noun = PartOfSpeech {
        guid: "pos-n".to_string(),
        name: "Noun".to_string(),
        abbreviation: "n".to_string(),
        children: Vec::new(),
        inflection_classes: Vec::new(),
        default_inflection_class: None,
        inflectable_features: Vec::new(),
        stem_names: Vec::new(),
        affix_slots: Vec::new(),
        affix_templates: Vec::new(),
    };
    let phonology = Phonology {
        phonemes: ["k", "a", "t", "p", "i", "u", "s", "o", "e"]
            .iter()
            .map(|p| Phoneme {
                guid: format!("ph-{p}"),
                name: p.to_string(),
                representations: vec![ws(p)],
                features: None,
                basic_ipa_symbol: None,
            })
            .collect(),
        boundary_markers: vec![BoundaryMarker {
            guid: "bd-plus".to_string(),
            name: "+".to_string(),
            representations: vec![ws("+")],
        }],
        ..Phonology::default()
    };
    let entries = vec![
        entry(
            "kat",
            MorphType::Root,
            "kat",
            vec![stem_msa("k1"), stem_msa("k2")],
            &["k2"],
        ),
        entry(
            "pit",
            MorphType::Root,
            "pit",
            vec![stem_msa("p1"), stem_msa("p2")],
            &[],
        ),
        entry(
            "tap",
            MorphType::Root,
            "tap",
            vec![stem_msa("t1"), stem_msa("t2")],
            &["t1", "t2"],
        ),
        entry(
            "u",
            MorphType::Suffix,
            "u",
            vec![affix_msa("u1"), affix_msa("u2")],
            &["u1", "u2"],
        ),
        entry(
            "s",
            MorphType::Suffix,
            "s",
            vec![affix_msa("s1"), affix_msa("s2")],
            &["s1", "s2"],
        ),
        variant("kot", MorphType::Root, "kot", "kat"),
        variant("kit", MorphType::Root, "kit", "sense-t2"),
        entry(
            "e",
            MorphType::Suffix,
            "e",
            vec![affix_msa("e1"), affix_msa("e2")],
            &["e1", "e2"],
        ),
        variant("o", MorphType::Suffix, "o", "e"),
    ];
    let pair = |a: &str, b: &str| {
        [
            EqualMsas {
                msa: a.to_string(),
                equal_to: vec![b.to_string()],
            },
            EqualMsas {
                msa: b.to_string(),
                equal_to: vec![a.to_string()],
            },
        ]
    };
    let equality: Vec<EqualMsas> = [
        pair("k1", "k2"),
        pair("p1", "p2"),
        pair("t1", "t2"),
        pair("u1", "u2"),
        pair("s1", "s2"),
        pair("e1", "e2"),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut morphology = Morphology {
        parts_of_speech: vec![noun],
        adhoc_prohibitions: vec![AdhocProhibition::Morpheme {
            guid: "prohibit-u-s".to_string(),
            disabled: false,
            primary: "u2".to_string(),
            others: vec!["s2".to_string(), "s1".to_string()],
            adjacency: Adjacency::Anywhere,
        }],
        ..Morphology::default()
    };
    morphology.parser_parameters.merge_msas = merge;
    Snapshot::new(
        Project {
            name: "Test".to_string(),
            vernacular_writing_systems: vec!["sen".to_string()],
            analysis_writing_systems: vec!["en".to_string()],
            exemplar_characters: Vec::new(),
        },
        FeatureSystems::default(),
        phonology,
        morphology,
        Lexicon {
            entries,
            msa_equality: extracted.then_some(equality),
        },
    )
}

/// Make exactly one FieldWorks equality fact: `candidate.EqualsMsa(lookup)`.
fn snapshot_with_tap_equality(candidate: &str, lookup: &str) -> Snapshot {
    let mut project = snapshot(true, true);
    let equalities = project
        .lexicon
        .msa_equality
        .as_mut()
        .expect("fixture has extracted MSA equality");
    equalities.retain(|equality| equality.msa != "t1" && equality.msa != "t2");
    equalities.push(EqualMsas {
        msa: lookup.to_string(),
        equal_to: vec![candidate.to_string()],
    });
    project
}

fn compile(merge: bool) -> pg_grammar::model::Grammar {
    pg_grammar::compile_project(&snapshot(merge, true))
        .expect("must compile")
        .0
}

/// Each analysis of `word` as its morphemes' source MSA guids, sorted for comparison.
fn analyses(grammar: &pg_grammar::model::Grammar, word: &str) -> Vec<Vec<String>> {
    let outcome = pg_parse::Morpher::new(grammar, usize::MAX).parse_word(word);
    let mut out: Vec<Vec<String>> = outcome
        .structured
        .iter()
        .map(|analysis| {
            analysis
                .morpheme_ids
                .iter()
                .map(|&id| {
                    grammar.morphemes[id as usize]
                        .source_msa_guid
                        .clone()
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect();
    out.sort();
    out
}

fn ids(rows: &[&[&str]]) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = rows
        .iter()
        .map(|row| row.iter().map(|s| s.to_string()).collect())
        .collect();
    out.sort();
    out
}

#[test]
fn own_stems_load_once_as_the_first_sense_bearing_equal_msa() {
    let merged = compile(true);
    let unmerged = compile(false);
    assert_eq!(analyses(&merged, "kat"), ids(&[&["k2"]]), "k1 has no sense");
    assert_eq!(analyses(&unmerged, "kat"), ids(&[&["k1"], &["k2"]]));
    assert_eq!(
        analyses(&merged, "tap"),
        ids(&[&["t1"]]),
        "first in entry order"
    );
    assert_eq!(analyses(&unmerged, "tap"), ids(&[&["t1"], &["t2"]]));
    assert_eq!(
        analyses(&merged, "pit"),
        ids(&[&["p1"], &["p2"]]),
        "an MSA no sense uses is its own representative"
    );
}

#[test]
fn own_affix_rules_load_once() {
    assert_eq!(analyses(&compile(true), "tapu"), ids(&[&["t1", "u1"]]));
    assert_eq!(
        analyses(&compile(false), "tapu").len(),
        4,
        "t1/t2 x u1/u2 unmerged"
    );
}

#[test]
fn variants_of_an_entry_and_of_a_sense_take_the_representative() {
    let merged = compile(true);
    let unmerged = compile(false);
    assert_eq!(analyses(&merged, "kot"), ids(&[&["k2"]]));
    assert_eq!(analyses(&unmerged, "kot"), ids(&[&["k1"], &["k2"]]));
    assert_eq!(
        analyses(&merged, "kit"),
        ids(&[&["t1"]]),
        "t2's sense maps to t1"
    );
    assert_eq!(analyses(&unmerged, "kit"), ids(&[&["t2"]]));
    assert_eq!(analyses(&merged, "tapo"), ids(&[&["t1", "e1"]]));
    assert_eq!(analyses(&unmerged, "tapo").len(), 4);
}

#[test]
fn an_adhoc_prohibition_on_merged_msas_applies_to_their_representatives() {
    let merged = analyses(&compile(true), "tapus");
    assert!(
        merged.is_empty(),
        "u2 and s2 map to the loaded u1 and s1, so the prohibition holds: {merged:?}"
    );
    let unmerged = analyses(&compile(false), "tapus");
    assert_eq!(
        unmerged.len(),
        8,
        "unmerged, the prohibition needs u2 with both s2 and s1: {unmerged:?}"
    );
}

#[test]
fn the_grammar_maps_every_merged_msa_to_its_representative() {
    let merged = compile(true);
    let pairs: BTreeSet<(&str, &str)> = merged
        .representative_msas
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    assert_eq!(
        pairs,
        BTreeSet::from([
            ("k1", "k2"),
            ("t2", "t1"),
            ("u2", "u1"),
            ("s2", "s1"),
            ("e2", "e1")
        ])
    );
    assert_eq!(merged.representative_msa("k1"), "k2");
    assert_eq!(merged.representative_msa("p1"), "p1");
    assert!(compile(false).representative_msas.is_empty());
}

#[test]
fn representative_selection_uses_the_candidate_equals_lookup_direction() {
    // The extracted row means t1.EqualsMsa(t2), so t2 resolves to the earlier, sense-bearing t1.
    let t1_equals_t2 = pg_grammar::compile_project(&snapshot_with_tap_equality("t1", "t2"))
        .expect("must compile")
        .0;
    assert_eq!(t1_equals_t2.representative_msa("t2"), "t1");
    assert_eq!(analyses(&t1_equals_t2, "tap"), ids(&[&["t1"]]));

    // Reversing only that one-way fact means t2 is not equal to the candidate t1 when t2 is looked up.
    let t2_equals_t1 = pg_grammar::compile_project(&snapshot_with_tap_equality("t2", "t1"))
        .expect("must compile")
        .0;
    assert_eq!(t2_equals_t1.representative_msa("t2"), "t2");
    assert_eq!(analyses(&t2_equals_t1, "tap"), ids(&[&["t1"], &["t2"]]));
}

#[test]
fn a_merged_msa_records_a_merged_load_decision() {
    let out = pg_grammar::compile_project_with(
        &snapshot(true, true),
        pg_grammar::compile::CompileOptions::default(),
    )
    .expect("must compile");
    let decision = out
        .load_decisions
        .iter()
        .find(|d| format!("{:?}", d.subject).contains("\"k1\""))
        .expect("k1 has a load decision");
    assert_eq!(decision.reason_code, pg_snapshot::LoadReasonCode::MergedMsa);
    assert_eq!(
        decision.effective_value_json.as_deref(),
        Some("{\"representativeMsa\":\"k2\"}")
    );
}

#[test]
fn a_snapshot_without_msa_equality_is_refused_only_when_merging_could_matter() {
    let err = pg_grammar::compile_project(&snapshot(true, false))
        .expect_err("merging is on and the snapshot cannot say which MSAs are equal");
    let message = err.to_string();
    assert!(message.contains("MergeMSAs"), "{message}");
    assert!(message.contains("re-import"), "{message}");
    assert!(message.contains("kat"), "{message}");
    assert!(pg_grammar::compile_project(&snapshot(false, false)).is_ok());
}
