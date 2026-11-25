//! `<HC><AffixRules>` maxApps loads into an affix rule's `multipleApplication` (C# `AffixProcessRule.MaxApplicationCount`, default 1).

use pg_snapshot::lexicon::{Allomorph, EqualMsas, LexEntry, Lexicon, Msa, Sense};
use pg_snapshot::morphology::{
    AffixRuleMaxApplications, AffixSlot, AffixTemplate, MorphType, Morphology, PartOfSpeech,
};
use pg_snapshot::phonology::{BoundaryMarker, Phoneme, Phonology};
use pg_snapshot::project::Project;
use pg_snapshot::{FeatureSystems, Snapshot, WsForm};

fn ws(form: &str) -> WsForm {
    WsForm {
        ws: "sen".to_string(),
        form: form.to_string(),
    }
}

fn entry(guid: &str, morph_type: MorphType, form: &str, msa: Msa) -> LexEntry {
    let msa_guid = msa.guid().to_string();
    LexEntry {
        guid: guid.to_string(),
        citation_form: vec![ws(form)],
        lexeme_morph_type: morph_type,
        allomorphs: vec![Allomorph {
            guid: format!("allo-{guid}"),
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
        }],
        msas: vec![msa],
        senses: vec![Sense {
            guid: format!("sense-{guid}"),
            gloss: vec![WsForm {
                ws: "en".to_string(),
                form: guid.to_uppercase(),
            }],
            definition: Vec::new(),
            msa: Some(msa_guid),
        }],
        entry_refs: Vec::new(),
    }
}

fn deriv(guid: &str) -> Msa {
    Msa::Derivational {
        guid: guid.to_string(),
        from_part_of_speech: Some("pos-v".to_string()),
        to_part_of_speech: Some("pos-v".to_string()),
        from_features: None,
        to_features: None,
        from_inflection_class: None,
        to_inflection_class: None,
        from_exception_features: Vec::new(),
        to_exception_features: Vec::new(),
        from_stem_name: None,
    }
}

/// Verb root `kat` with derivational suffixes -ish (`msa-caus`) and -el (`msa-appl`); `counts` is the authored `<HC><AffixRules>` content.
fn snapshot(counts: &[(&str, u32)]) -> Snapshot {
    let verb = PartOfSpeech {
        guid: "pos-v".to_string(),
        name: "Verb".to_string(),
        abbreviation: "v".to_string(),
        children: Vec::new(),
        inflection_classes: Vec::new(),
        default_inflection_class: None,
        inflectable_features: Vec::new(),
        stem_names: Vec::new(),
        affix_slots: Vec::new(),
        affix_templates: Vec::new(),
    };
    let phonology = Phonology {
        phonemes: ["k", "a", "t", "i", "s", "h", "e", "l"]
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
    let mut morphology = Morphology {
        parts_of_speech: vec![verb],
        ..Morphology::default()
    };
    morphology.parser_parameters.affix_rule_max_applications = counts
        .iter()
        .map(|(msa, n)| AffixRuleMaxApplications {
            msa: msa.to_string(),
            max_applications: *n,
        })
        .collect();
    let root = Msa::Stem {
        guid: "msa-kat".to_string(),
        part_of_speech: Some("pos-v".to_string()),
        inflection_class: None,
        features: None,
        exception_features: Vec::new(),
        from_parts_of_speech: Vec::new(),
        slots: Vec::new(),
    };
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
            entries: vec![
                entry("kat", MorphType::Root, "kat", root),
                entry("caus", MorphType::Suffix, "ish", deriv("msa-caus")),
                entry("appl", MorphType::Suffix, "el", deriv("msa-appl")),
            ],
            msa_equality: None,
        },
    )
}

fn parse_count(counts: &[(&str, u32)], word: &str) -> usize {
    let grammar = pg_grammar::compile_project(&snapshot(counts))
        .expect("must compile")
        .0;
    pg_parse::Morpher::new(&grammar, usize::MAX)
        .parse_word(word)
        .structured
        .len()
}

#[test]
fn an_authored_count_lets_the_affix_apply_that_many_times() {
    let counts = [("msa-caus", 2), ("msa-appl", 2)];
    assert_eq!(parse_count(&counts, "katish"), 1);
    assert_eq!(parse_count(&counts, "katishish"), 1);
    assert_eq!(parse_count(&counts, "katelel"), 1);
    assert_eq!(parse_count(&counts, "katishishelel"), 1);
}

#[test]
fn the_count_is_a_ceiling_not_unbounded() {
    let counts = [("msa-caus", 2), ("msa-appl", 2)];
    assert_eq!(parse_count(&counts, "katishishish"), 0);
    assert_eq!(parse_count(&counts, "katelelel"), 0);
}

#[test]
fn absent_count_means_one_application() {
    assert_eq!(parse_count(&[], "katish"), 1);
    assert_eq!(parse_count(&[], "katishish"), 0);
    assert_eq!(parse_count(&[], "katelel"), 0);
}

#[test]
fn a_count_applies_only_to_its_own_msa() {
    let counts = [("msa-caus", 2)];
    assert_eq!(parse_count(&counts, "katishish"), 1);
    assert_eq!(parse_count(&counts, "katelel"), 0);
}

fn parse_snapshot(snapshot: &Snapshot, word: &str) -> usize {
    let grammar = pg_grammar::compile_project(snapshot)
        .expect("must compile")
        .0;
    pg_parse::Morpher::new(&grammar, usize::MAX)
        .parse_word(word)
        .structured
        .len()
}

/// `-ish` carries two equal MSAs; only `msa-caus` has a sense, so it is the merged representative.
fn merged_snapshot(counts: &[(&str, u32)]) -> Snapshot {
    let mut s = snapshot(counts);
    let caus = s
        .lexicon
        .entries
        .iter_mut()
        .find(|e| e.guid == "caus")
        .unwrap();
    caus.msas.push(deriv("msa-caus-b"));
    let eq = |a: &str, b: &str| EqualMsas {
        msa: a.to_string(),
        equal_to: vec![b.to_string()],
    };
    s.lexicon.msa_equality = Some(vec![
        eq("msa-caus", "msa-caus-b"),
        eq("msa-caus-b", "msa-caus"),
    ]);
    s
}

#[test]
fn a_merged_msa_takes_the_count_authored_on_its_representative_only() {
    assert_eq!(
        parse_snapshot(&merged_snapshot(&[("msa-caus", 2)]), "katishish"),
        1
    );
    assert_eq!(
        parse_snapshot(&merged_snapshot(&[("msa-caus-b", 2)]), "katishish"),
        0,
        "a count on the non-representative equal MSA is never looked up"
    );
}

/// `-el` becomes an inflectional affix filling the only slot of a verb template.
fn template_snapshot(counts: &[(&str, u32)]) -> Snapshot {
    let mut s = snapshot(counts);
    s.morphology.parts_of_speech[0].affix_slots = vec![AffixSlot {
        guid: "slot-1".to_string(),
        name: "slot".to_string(),
        optional: false,
    }];
    s.morphology.parts_of_speech[0].affix_templates = vec![AffixTemplate {
        guid: "tpl-1".to_string(),
        name: "tpl".to_string(),
        disabled: false,
        prefix_slots: Vec::new(),
        suffix_slots: vec!["slot-1".to_string()],
        is_final: true,
    }];
    let appl = s
        .lexicon
        .entries
        .iter_mut()
        .find(|e| e.guid == "appl")
        .unwrap();
    appl.msas = vec![Msa::Inflectional {
        guid: "msa-appl".to_string(),
        part_of_speech: Some("pos-v".to_string()),
        slots: vec!["slot-1".to_string()],
        features: None,
        exception_features: Vec::new(),
    }];
    s
}

#[test]
fn a_template_slot_rule_is_visited_once_whatever_its_count() {
    assert_eq!(parse_snapshot(&template_snapshot(&[]), "katel"), 1);
    assert_eq!(
        parse_snapshot(&template_snapshot(&[("msa-appl", 2)]), "katelel"),
        0,
        "a slot applies its rule once; the count does not repeat the slot"
    );
}

#[test]
fn a_count_above_sixty_five_thousand_is_not_capped() {
    let grammar = pg_grammar::compile_project(&snapshot(&[("msa-caus", 2_147_483_647)]))
        .expect("must compile")
        .0;
    assert!(grammar.mrules.iter().any(|r| r.max_apps() == 2_147_483_647));
}

#[test]
fn with_merge_msas_off_each_equal_msa_applies_its_own_count() {
    let unmerged = |counts: &[(&str, u32)]| {
        let mut s = merged_snapshot(counts);
        s.morphology.parser_parameters.merge_msas = false;
        s
    };
    assert_eq!(parse_snapshot(&unmerged(&[]), "katishishish"), 0);
    assert!(parse_snapshot(&unmerged(&[("msa-caus-b", 2)]), "katishishish") > 0);
    assert_eq!(
        parse_snapshot(&merged_snapshot(&[("msa-caus-b", 2)]), "katishishish"),
        0,
        "merged, the non-representative count is dropped"
    );
}
