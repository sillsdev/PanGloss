//! `--without` on grammar rules (#397): a phonological rule, compound rule, affix template or ad hoc prohibition left out parses exactly as a grammar with that rule deleted from the snapshot; other objects are refused with their kind (synthetic grammar).

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");

use pg_snapshot::lexicon::{Allomorph, LexEntry, Lexicon, Msa, Sense};
use pg_snapshot::morphology::{
    AdhocProhibition, Adjacency, AffixSlot, AffixTemplate, CompoundConstituentRequirement,
    CompoundOutcome, CompoundRule, MorphType, Morphology, PartOfSpeech,
};
use pg_snapshot::phonology::{
    BoundaryMarker, PhonContext, Phoneme, PhonologicalRule, Phonology, RewriteRhs, RewriteRule,
    RuleDirection,
};
use pg_snapshot::project::Project;
use pg_snapshot::{FeatureSystems, Snapshot, WsForm};

fn ws(form: &str) -> WsForm {
    WsForm {
        ws: "sen".to_string(),
        form: form.to_string(),
    }
}

fn phoneme(rep: &str) -> Phoneme {
    Phoneme {
        guid: format!("ph-{rep}"),
        name: rep.to_string(),
        representations: vec![ws(rep)],
        features: None,
        basic_ipa_symbol: None,
    }
}

fn allomorph(guid: &str, morph_type: MorphType, form: &str, envs: &[&str]) -> Allomorph {
    Allomorph {
        guid: guid.to_string(),
        morph_type,
        is_abstract: false,
        forms: vec![ws(form)],
        environments: envs.iter().map(|e| e.to_string()).collect(),
        positions: Vec::new(),
        stem_name: None,
        inflection_classes: Vec::new(),
        ms_env_features: None,
        ms_env_part_of_speech: None,
        process: None,
    }
}

fn stem_entry(n: &str, form: &str) -> LexEntry {
    LexEntry {
        guid: format!("entry-{n}"),
        citation_form: vec![ws(form)],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![allomorph(&format!("allo-{n}"), MorphType::Stem, form, &[])],
        msas: vec![Msa::Stem {
            guid: format!("msa-{n}"),
            part_of_speech: Some("pos-verb".to_string()),
            inflection_class: None,
            features: None,
            exception_features: Vec::new(),
            from_parts_of_speech: Vec::new(),
            slots: Vec::new(),
        }],
        senses: vec![Sense {
            guid: format!("sense-{n}"),
            gloss: vec![WsForm {
                ws: "en".to_string(),
                form: n.to_string(),
            }],
            definition: Vec::new(),
            msa: Some(format!("msa-{n}")),
        }],
        entry_refs: Vec::new(),
    }
}

fn affix_entry(n: &str, form: &str, slots: &[&str]) -> LexEntry {
    LexEntry {
        guid: format!("entry-{n}"),
        citation_form: vec![ws(&format!("-{form}"))],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![allomorph(
            &format!("allo-{n}"),
            MorphType::Suffix,
            form,
            &[],
        )],
        msas: vec![Msa::Inflectional {
            guid: format!("msa-{n}"),
            part_of_speech: Some("pos-verb".to_string()),
            slots: slots.iter().map(|s| s.to_string()).collect(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: vec![Sense {
            guid: format!("sense-{n}"),
            gloss: vec![WsForm {
                ws: "en".to_string(),
                form: n.to_uppercase(),
            }],
            definition: Vec::new(),
            msa: Some(format!("msa-{n}")),
        }],
        entry_refs: Vec::new(),
    }
}

fn segment(p: &str) -> PhonContext {
    PhonContext::Segment {
        phoneme: format!("ph-{p}"),
    }
}

/// Stems kat, ma, ko; suffix -i in a template slot, suffix -u outside any template; the rule t -> s before i; a compound rule; the prohibition ma + -u.
fn snapshot() -> Snapshot {
    let verb = PartOfSpeech {
        guid: "pos-verb".to_string(),
        name: "Verb".to_string(),
        abbreviation: "v".to_string(),
        children: Vec::new(),
        inflection_classes: Vec::new(),
        default_inflection_class: None,
        inflectable_features: Vec::new(),
        stem_names: Vec::new(),
        affix_slots: vec![AffixSlot {
            guid: "slot-one".to_string(),
            name: "One".to_string(),
            optional: true,
        }],
        affix_templates: vec![AffixTemplate {
            guid: "tmpl-verb".to_string(),
            name: "VerbTemplate".to_string(),
            disabled: false,
            prefix_slots: Vec::new(),
            suffix_slots: vec!["slot-one".to_string()],
            is_final: true,
        }],
    };
    let phonology = Phonology {
        phonemes: ["k", "t", "s", "m", "a", "i", "u", "o"]
            .into_iter()
            .map(phoneme)
            .collect(),
        boundary_markers: vec![BoundaryMarker {
            guid: "bd-plus".to_string(),
            name: "+".to_string(),
            representations: vec![ws("+")],
        }],
        rules: vec![PhonologicalRule::Rewrite(RewriteRule {
            guid: "prule-t-s".to_string(),
            name: "t to s before i".to_string(),
            direction: RuleDirection::Simultaneous,
            structural_description: vec![segment("t")],
            feature_constraint_variables: Vec::new(),
            right_hand_sides: vec![RewriteRhs {
                structural_change: vec![segment("s")],
                right_context: Some(segment("i")),
                ..RewriteRhs::default()
            }],
        })],
        ..Phonology::default()
    };
    let mut morphology = Morphology {
        parts_of_speech: vec![verb],
        ..Morphology::default()
    };
    morphology.compound_rules.push(CompoundRule::Endocentric {
        guid: "crule-vv".to_string(),
        name: "VV".to_string(),
        disabled: false,
        head_last: true,
        left: CompoundConstituentRequirement::default(),
        right: CompoundConstituentRequirement::default(),
        overriding: CompoundOutcome::default(),
    });
    morphology
        .adhoc_prohibitions
        .push(AdhocProhibition::Morpheme {
            guid: "coocc-ma-u".to_string(),
            disabled: false,
            primary: "msa-ma".to_string(),
            others: vec!["msa-u".to_string()],
            adjacency: Adjacency::Anywhere,
        });
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
            msa_equality: None,
            entries: vec![
                stem_entry("kat", "kat"),
                stem_entry("ma", "ma"),
                stem_entry("ko", "ko"),
                affix_entry("i", "i", &["slot-one"]),
                affix_entry("u", "u", &[]),
            ],
        },
    )
}

/// The snapshot JSON with the object whose guid is `guid` deleted by editing the document, not by the code under test.
fn deleted_json(snapshot: &Snapshot, section: &[&str], guid: &str) -> String {
    let mut value: Value = serde_json::from_str(&snapshot.to_json()).unwrap();
    let mut node = &mut value;
    for key in section {
        node = &mut node[*key];
    }
    node.as_array_mut().unwrap().retain(|o| o["guid"] != guid);
    serde_json::to_string(&value).unwrap()
}

fn parse(grammar: &Path, word: &str, without: Option<&str>) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.arg("parse").arg(grammar).arg(word);
    if let Some(w) = without {
        cmd.args(["--without", w]);
    }
    cmd.output().unwrap()
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Each word parses the same with `--without guid` as in a grammar with the rule deleted, and at least one word differs from the plain parse.
fn assert_equals_deletion(section: &[&str], guid: &str, words: &[&str]) {
    let dir = tempfile::tempdir().unwrap();
    let snapshot = snapshot();
    let original = dir.path().join("grammar.json");
    std::fs::write(&original, snapshot.to_json()).unwrap();
    let deleted = dir.path().join("deleted.json");
    std::fs::write(&deleted, deleted_json(&snapshot, section, guid)).unwrap();
    let mut changed = false;
    for word in words {
        let plain = stdout(&parse(&original, word, None));
        let without = stdout(&parse(&original, word, Some(guid)));
        let expected = stdout(&parse(&deleted, word, None));
        let strip = |s: &str| {
            s.lines()
                .map(|l| l.to_string())
                .filter(|l| !l.contains("grammar.json") && !l.contains("deleted.json"))
                .collect::<Vec<_>>()
        };
        assert_eq!(strip(&without), strip(&expected), "{word} without {guid}");
        changed |= strip(&without) != strip(&plain);
    }
    assert!(changed, "leaving out {guid} must change some of {words:?}");
}

#[test]
fn without_a_phonological_rule_equals_deleting_it() {
    assert_equals_deletion(&["phonology", "rules"], "prule-t-s", &["kasi", "kati"]);
}

#[test]
fn without_a_compound_rule_equals_deleting_it() {
    assert_equals_deletion(
        &["morphology", "compoundRules"],
        "crule-vv",
        &["katko", "koma"],
    );
}

#[test]
fn without_an_affix_template_equals_deleting_it() {
    let dir = tempfile::tempdir().unwrap();
    let snapshot = snapshot();
    let original = dir.path().join("grammar.json");
    std::fs::write(&original, snapshot.to_json()).unwrap();
    let mut value: Value = serde_json::from_str(&snapshot.to_json()).unwrap();
    value["morphology"]["partsOfSpeech"][0]["affixTemplates"] = Value::Array(Vec::new());
    let deleted = dir.path().join("deleted.json");
    std::fs::write(&deleted, serde_json::to_string(&value).unwrap()).unwrap();
    let mut changed = false;
    for word in ["koi", "kou", "ko"] {
        let plain = stdout(&parse(&original, word, None));
        let without = stdout(&parse(&original, word, Some("tmpl-verb")));
        let expected = stdout(&parse(&deleted, word, None));
        let strip = |s: &str| s.lines().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(strip(&without), strip(&expected), "{word}");
        changed |= without != plain;
    }
    assert!(changed, "the template must matter to some word");
}

#[test]
fn without_an_adhoc_prohibition_equals_deleting_it() {
    assert_equals_deletion(
        &["morphology", "adhocProhibitions"],
        "coocc-ma-u",
        &["mau", "kou"],
    );
}

#[test]
fn without_refuses_unsupported_objects_with_their_kind() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("grammar.json");
    std::fs::write(&original, snapshot().to_json()).unwrap();
    for (guid, expect) in [
        ("slot-one", "affix template slot"),
        ("pos-verb", "part of speech"),
        ("ph-t", "phoneme"),
        ("msa-ma", "morphosyntactic analysis"),
        ("sense-ma", "sense"),
        ("nowhere", "neither an entry"),
    ] {
        let output = parse(&original, "kat", Some(guid));
        assert!(!output.status.success(), "{guid} must be refused");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(guid) && stderr.contains(expect),
            "{guid}: {stderr}"
        );
    }
}
