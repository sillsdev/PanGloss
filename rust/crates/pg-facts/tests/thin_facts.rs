use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use pg_facts::{FactsError, ProducerIdentity};
use pg_snapshot::feature::{
    ClosedFeature, FeatureStructure, FeatureSystems, FeatureValue, FeatureValueKind,
    FeatureValueSymbol,
};
use pg_snapshot::lexicon::{Allomorph, LexEntry, Lexicon, Msa, Sense};
use pg_snapshot::morphology::{
    AdhocProhibition, Adjacency, AffixSlot, AffixTemplate, CompoundOutcome, CompoundRule,
    MorphType, Morphology, PartOfSpeech,
};
use pg_snapshot::phonology::{
    BoundaryMarker, Environment, NaturalClass, PhonContext, Phoneme, PhonologicalRule, Phonology,
    RewriteRhs, RewriteRule, RuleDirection,
};
use pg_snapshot::project::Project;
use pg_snapshot::{Snapshot, WsForm};
use pg_stats::{
    FactRecord, IdentityQuality, ObjectKind, RunMetadata, StatsCache, StepCap, StructuralLocator,
    WordRecord,
};
use rusqlite::Connection;

const POS: &str = "00000000-0000-0000-0000-000000000001";
const SLOT: &str = "00000000-0000-0000-0000-000000000002";
const TEMPLATE: &str = "00000000-0000-0000-0000-000000000003";
const SLOT_TWO: &str = "00000000-0000-0000-0000-000000000004";
const SLOT_THREE: &str = "00000000-0000-0000-0000-000000000005";
const SLOT_EMPTY: &str = "00000000-0000-0000-0000-000000000006";
const TEMPLATE_DISABLED: &str = "00000000-0000-0000-0000-000000000007";
const TEMPLATE_EMPTY: &str = "00000000-0000-0000-0000-000000000008";
const TEMPLATE_NO_RULES: &str = "00000000-0000-0000-0000-000000000009";
const TEMPLATE_SHARED: &str = "00000000-0000-0000-0000-000000000016";
const ENTRY_ROOT: &str = "00000000-0000-0000-0000-000000000010";
const ENTRY_ROOT_TWO: &str = "00000000-0000-0000-0000-000000000011";
const ENTRY_SUFFIX: &str = "00000000-0000-0000-0000-000000000012";
const SENSE_ROOT: &str = "00000000-0000-0000-0000-000000000013";
const SENSE_ROOT_TWO: &str = "00000000-0000-0000-0000-000000000014";
const SENSE_SUFFIX: &str = "00000000-0000-0000-0000-000000000015";
const MSA_ROOT: &str = "00000000-0000-0000-0000-000000000020";
const MSA_ROOT_TWO: &str = "00000000-0000-0000-0000-000000000021";
const MSA_SUFFIX: &str = "00000000-0000-0000-0000-000000000022";
const MSA_UNSLOTTED: &str = "00000000-0000-0000-0000-000000000023";
const ALLO_ROOT: &str = "00000000-0000-0000-0000-000000000030";
const ALLO_ROOT_TWO: &str = "00000000-0000-0000-0000-000000000031";
const ALLO_SUFFIX: &str = "00000000-0000-0000-0000-000000000032";
const ALLO_ROOT_ALT: &str = "00000000-0000-0000-0000-000000000033";
const ALLO_ROOT_ABSTRACT: &str = "00000000-0000-0000-0000-000000000034";
const ALLO_ROOT_EMPTY: &str = "00000000-0000-0000-0000-000000000035";
const ENTRY_CIRCUMFIX: &str = "00000000-0000-0000-0000-0000000000a1";
const MSA_CIRCUMFIX: &str = "00000000-0000-0000-0000-0000000000a2";
const ALLO_CIRCUMFIX_PREFIX: &str = "00000000-0000-0000-0000-0000000000a3";
const ALLO_CIRCUMFIX_SUFFIX: &str = "00000000-0000-0000-0000-0000000000a4";
const COMPOUND_RULE: &str = "00000000-0000-0000-0000-0000000000a5";
const POS_VERB: &str = "00000000-0000-0000-0000-000000000050";
const T2_SLOT_CLASS: &str = "00000000-0000-0000-0000-000000000051";
const T2_SLOT_NUMBER: &str = "00000000-0000-0000-0000-000000000052";
const T2_SLOT_CASE: &str = "00000000-0000-0000-0000-000000000053";
const T2_SLOT_TENSE: &str = "00000000-0000-0000-0000-000000000054";
const T2_SLOT_PERSON: &str = "00000000-0000-0000-0000-000000000055";
const T2_TEMPLATE_NOUN: &str = "00000000-0000-0000-0000-000000000056";
const T2_TEMPLATE_VERB: &str = "00000000-0000-0000-0000-000000000057";
const T2_ENTRY_NOUN_ROOT: &str = "00000000-0000-0000-0000-000000000058";
const T2_ENTRY_VERB_ROOT: &str = "00000000-0000-0000-0000-000000000059";
const T2_ENTRY_CLASS: &str = "00000000-0000-0000-0000-000000000060";
const T2_ENTRY_NUMBER: &str = "00000000-0000-0000-0000-000000000061";
const T2_ENTRY_CASE: &str = "00000000-0000-0000-0000-000000000062";
const T2_ENTRY_TENSE: &str = "00000000-0000-0000-0000-000000000063";
const T2_ENTRY_PERSON: &str = "00000000-0000-0000-0000-000000000064";
const T2_MSA_CLASS: &str = "00000000-0000-0000-0000-000000000065";
const T2_MSA_NUMBER: &str = "00000000-0000-0000-0000-000000000066";
const T2_MSA_CASE: &str = "00000000-0000-0000-0000-000000000067";
const T2_MSA_TENSE: &str = "00000000-0000-0000-0000-000000000068";
const T2_MSA_PERSON: &str = "00000000-0000-0000-0000-000000000069";
const T2_ALLO_CLASS_A: &str = "00000000-0000-0000-0000-000000000070";
const T2_ALLO_CLASS_B: &str = "00000000-0000-0000-0000-000000000071";
const T2_ALLO_NUMBER: &str = "00000000-0000-0000-0000-000000000072";
const T2_ALLO_CASE: &str = "00000000-0000-0000-0000-000000000073";
const T2_ALLO_TENSE_VOWEL: &str = "00000000-0000-0000-0000-000000000074";
const T2_ALLO_TENSE_CONSONANT: &str = "00000000-0000-0000-0000-000000000075";
const T2_ALLO_PERSON: &str = "00000000-0000-0000-0000-000000000076";
const PROHIBITION_MORPHEME: &str = "00000000-0000-0000-0000-000000000040";
const PROHIBITION_ALLOMORPH: &str = "00000000-0000-0000-0000-000000000041";
const PROHIBITION_DISABLED: &str = "00000000-0000-0000-0000-000000000042";
const PROHIBITION_COMPACTED_TARGET: &str = "00000000-0000-0000-0000-00000000004a";
const ENTRY_ORPHAN: &str = "00000000-0000-0000-0000-00000000004b";
const SENSE_ORPHAN: &str = "00000000-0000-0000-0000-00000000004c";
const MSA_ORPHAN: &str = "00000000-0000-0000-0000-00000000004d";
const ALLO_ORPHAN: &str = "00000000-0000-0000-0000-00000000004e";
const ENV_VALID: &str = "00000000-0000-0000-0000-000000000090";
const ENV_REPEAT: &str = "00000000-0000-0000-0000-000000000091";
const ENV_INVALID: &str = "00000000-0000-0000-0000-000000000092";
const ENV_UNUSED: &str = "00000000-0000-0000-0000-000000000093";
const ENV_DANGLING: &str = "00000000-0000-0000-0000-000000000094";
const ENV_RIGHT_ANCHOR: &str = "00000000-0000-0000-0000-000000000100";
const NC_C: &str = "00000000-0000-0000-0000-000000000095";
const NC_C_DUP: &str = "00000000-0000-0000-0000-000000000096";
const NC_VOICED: &str = "00000000-0000-0000-0000-000000000097";
const FEATURE_VOICE: &str = "00000000-0000-0000-0000-000000000098";
const VALUE_PLUS: &str = "00000000-0000-0000-0000-000000000099";
const VALUE_MINUS: &str = "00000000-0000-0000-0000-00000000009a";
const ALLO_OWNER_REJECTED: &str = "00000000-0000-0000-0000-00000000009b";
const ENTRY_OWNER_REJECTED: &str = "00000000-0000-0000-0000-00000000009c";
const MSA_OWNER_REJECTED: &str = "00000000-0000-0000-0000-00000000009d";
const SENSE_OWNER_REJECTED: &str = "00000000-0000-0000-0000-00000000009e";
const RULE_REWRITE: &str = "00000000-0000-0000-0000-00000000009f";

fn ws(ws: &str, form: &str) -> WsForm {
    WsForm {
        ws: ws.into(),
        form: form.into(),
    }
}

fn allomorph(guid: &str, morph_type: MorphType, form: &str) -> Allomorph {
    Allomorph {
        guid: guid.into(),
        morph_type,
        is_abstract: false,
        forms: vec![ws("qaa", form)],
        environments: Vec::new(),
        positions: Vec::new(),
        stem_name: None,
        inflection_classes: Vec::new(),
        ms_env_features: None,
        ms_env_part_of_speech: None,
        process: None,
    }
}

fn entry(guid: &str, sense_guid: &str, allo: Allomorph, msa: Msa, gloss: &str) -> LexEntry {
    let msa_guid = msa.guid().to_string();
    LexEntry {
        guid: guid.into(),
        citation_form: Vec::new(),
        lexeme_morph_type: allo.morph_type,
        allomorphs: vec![allo],
        msas: vec![msa],
        senses: vec![Sense {
            guid: sense_guid.into(),
            gloss: vec![ws("en", gloss)],
            definition: Vec::new(),
            msa: Some(msa_guid),
        }],
        entry_refs: Vec::new(),
    }
}

fn t2_snapshot() -> Snapshot {
    // Mirrors the T2 lomi gold's ordinary suffix slots and affix forms.
    let mut source = snapshot();
    let slots = |category_guid: &str, values: &[(&str, &str, bool)]| PartOfSpeech {
        guid: category_guid.into(),
        name: if category_guid == POS { "Noun" } else { "Verb" }.into(),
        abbreviation: if category_guid == POS { "n" } else { "v" }.into(),
        children: Vec::new(),
        inflection_classes: Vec::new(),
        default_inflection_class: None,
        inflectable_features: Vec::new(),
        stem_names: Vec::new(),
        affix_slots: values
            .iter()
            .map(|(guid, name, optional)| AffixSlot {
                guid: (*guid).into(),
                name: (*name).into(),
                optional: *optional,
            })
            .collect(),
        affix_templates: Vec::new(),
    };
    let mut noun = slots(
        POS,
        &[
            (T2_SLOT_CLASS, "Noun class", false),
            (T2_SLOT_NUMBER, "Number", true),
            (T2_SLOT_CASE, "Case", true),
        ],
    );
    noun.affix_templates.push(AffixTemplate {
        guid: T2_TEMPLATE_NOUN.into(),
        name: "Noun suffixes".into(),
        disabled: false,
        prefix_slots: Vec::new(),
        suffix_slots: vec![
            T2_SLOT_CLASS.into(),
            T2_SLOT_NUMBER.into(),
            T2_SLOT_CASE.into(),
        ],
        is_final: true,
    });
    let mut verb = slots(
        POS_VERB,
        &[
            (T2_SLOT_TENSE, "Tense", true),
            (T2_SLOT_PERSON, "Person", true),
        ],
    );
    verb.affix_templates.push(AffixTemplate {
        guid: T2_TEMPLATE_VERB.into(),
        name: "Verb suffixes".into(),
        disabled: false,
        prefix_slots: Vec::new(),
        suffix_slots: vec![T2_SLOT_TENSE.into(), T2_SLOT_PERSON.into()],
        is_final: true,
    });
    source.morphology.parts_of_speech = vec![noun, verb];
    source.morphology.adhoc_prohibitions.clear();

    let msa = |guid: &str, category: &str, slot: &str| Msa::Inflectional {
        guid: guid.into(),
        part_of_speech: Some(category.into()),
        slots: vec![slot.into()],
        features: None,
        exception_features: Vec::new(),
    };
    let mut class_marker = entry(
        T2_ENTRY_CLASS,
        "00000000-0000-0000-0000-000000000077",
        allomorph(T2_ALLO_CLASS_A, MorphType::Suffix, "na"),
        msa(T2_MSA_CLASS, POS, T2_SLOT_CLASS),
        "NCL",
    );
    class_marker
        .allomorphs
        .push(allomorph(T2_ALLO_CLASS_B, MorphType::Suffix, "ta"));
    let mut tense = entry(
        T2_ENTRY_TENSE,
        "00000000-0000-0000-0000-000000000078",
        allomorph(T2_ALLO_TENSE_VOWEL, MorphType::Suffix, "ka"),
        msa(T2_MSA_TENSE, POS_VERB, T2_SLOT_TENSE),
        "PST",
    );
    tense
        .allomorphs
        .push(allomorph(T2_ALLO_TENSE_CONSONANT, MorphType::Suffix, "uka"));
    source.lexicon.entries = vec![
        entry(
            T2_ENTRY_NOUN_ROOT,
            "00000000-0000-0000-0000-000000000079",
            allomorph(
                "00000000-0000-0000-0000-000000000080",
                MorphType::Root,
                "kuma",
            ),
            Msa::Stem {
                guid: "00000000-0000-0000-0000-000000000081".into(),
                part_of_speech: Some(POS.into()),
                inflection_class: None,
                features: None,
                exception_features: Vec::new(),
                from_parts_of_speech: Vec::new(),
                slots: Vec::new(),
            },
            "sandal",
        ),
        entry(
            T2_ENTRY_VERB_ROOT,
            "00000000-0000-0000-0000-000000000082",
            allomorph(
                "00000000-0000-0000-0000-000000000083",
                MorphType::Root,
                "aga",
            ),
            Msa::Stem {
                guid: "00000000-0000-0000-0000-000000000084".into(),
                part_of_speech: Some(POS_VERB.into()),
                inflection_class: None,
                features: None,
                exception_features: Vec::new(),
                from_parts_of_speech: Vec::new(),
                slots: Vec::new(),
            },
            "to swim",
        ),
        class_marker,
        entry(
            T2_ENTRY_NUMBER,
            "00000000-0000-0000-0000-000000000085",
            allomorph(T2_ALLO_NUMBER, MorphType::Suffix, "ri"),
            msa(T2_MSA_NUMBER, POS, T2_SLOT_NUMBER),
            "PL",
        ),
        entry(
            T2_ENTRY_CASE,
            "00000000-0000-0000-0000-000000000086",
            allomorph(T2_ALLO_CASE, MorphType::Suffix, "de"),
            msa(T2_MSA_CASE, POS, T2_SLOT_CASE),
            "LOC",
        ),
        tense,
        entry(
            T2_ENTRY_PERSON,
            "00000000-0000-0000-0000-000000000087",
            allomorph(T2_ALLO_PERSON, MorphType::Suffix, "mi"),
            msa(T2_MSA_PERSON, POS_VERB, T2_SLOT_PERSON),
            "1SG",
        ),
    ];
    for entry in &mut source.lexicon.entries {
        for allomorph in &mut entry.allomorphs {
            for form in &mut allomorph.forms {
                form.ws = "qac".into();
            }
        }
    }
    source.phonology.phonemes = ["a", "d", "e", "g", "i", "k", "m", "n", "r", "t", "u"]
        .into_iter()
        .enumerate()
        .map(|(index, representation)| Phoneme {
            guid: format!("00000000-0000-0000-0000-0000000001{:02}", index + 20),
            name: representation.into(),
            representations: vec![ws("qac", representation)],
            features: None,
            basic_ipa_symbol: None,
        })
        .collect();
    source.project.vernacular_writing_systems = vec!["qac".into()];
    source.project.name = "T2 lomi projection fixture".into();
    source
}

fn t2_parse_signatures(snapshot: &Snapshot) -> BTreeMap<String, String> {
    let (grammar, _) = pg_grammar::compile_project(snapshot).expect("T2 snapshot compiles");
    let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
    [
        "kuma",
        "kumana",
        "kumanari",
        "kumanade",
        "kumanaride",
        "agaka",
        "agami",
        "agakami",
        "agukami",
    ]
    .into_iter()
    .map(|word| (word.into(), morpher.parse_word(word).signature()))
    .collect()
}

fn snapshot() -> Snapshot {
    let pos = PartOfSpeech {
        guid: POS.into(),
        name: "Noun".into(),
        abbreviation: "n".into(),
        children: Vec::new(),
        inflection_classes: Vec::new(),
        default_inflection_class: None,
        inflectable_features: Vec::new(),
        stem_names: Vec::new(),
        affix_slots: vec![AffixSlot {
            guid: SLOT.into(),
            name: "Plural".into(),
            optional: false,
        }],
        affix_templates: vec![AffixTemplate {
            guid: TEMPLATE.into(),
            name: "Noun template".into(),
            disabled: false,
            prefix_slots: Vec::new(),
            suffix_slots: vec![SLOT.into()],
            is_final: true,
        }],
    };
    let root_msa = Msa::Stem {
        guid: MSA_ROOT.into(),
        part_of_speech: Some(POS.into()),
        inflection_class: None,
        features: None,
        exception_features: Vec::new(),
        from_parts_of_speech: Vec::new(),
        slots: Vec::new(),
    };
    let root_two_msa = Msa::Stem {
        guid: MSA_ROOT_TWO.into(),
        part_of_speech: Some(POS.into()),
        inflection_class: None,
        features: None,
        exception_features: Vec::new(),
        from_parts_of_speech: Vec::new(),
        slots: Vec::new(),
    };
    let suffix_msa = Msa::Inflectional {
        guid: MSA_SUFFIX.into(),
        part_of_speech: Some(POS.into()),
        slots: vec![SLOT.into()],
        features: None,
        exception_features: Vec::new(),
    };
    let unslotted_msa = Msa::Inflectional {
        guid: MSA_UNSLOTTED.into(),
        part_of_speech: Some(POS.into()),
        slots: Vec::new(),
        features: None,
        exception_features: Vec::new(),
    };
    let mut suffix_entry = entry(
        ENTRY_SUFFIX,
        SENSE_SUFFIX,
        allomorph(ALLO_SUFFIX, MorphType::Suffix, "ta"),
        suffix_msa,
        "plural",
    );
    suffix_entry.msas.push(unslotted_msa);
    let mut root_entry = entry(
        ENTRY_ROOT,
        SENSE_ROOT,
        allomorph(ALLO_ROOT, MorphType::Root, "kuma"),
        root_msa,
        "root one",
    );
    root_entry.citation_form = vec![ws("qaa", "kuma")];
    let mut morphology = Morphology {
        parts_of_speech: vec![pos],
        adhoc_prohibitions: vec![
            AdhocProhibition::Morpheme {
                guid: PROHIBITION_MORPHEME.into(),
                disabled: false,
                primary: MSA_ROOT.into(),
                others: vec![MSA_ROOT_TWO.into(), MSA_SUFFIX.into()],
                adjacency: Adjacency::SomewhereToRight,
            },
            AdhocProhibition::Allomorph {
                guid: PROHIBITION_ALLOMORPH.into(),
                disabled: false,
                primary: ALLO_ROOT.into(),
                others: vec![ALLO_ROOT_TWO.into(), ALLO_SUFFIX.into()],
                adjacency: Adjacency::AdjacentToLeft,
            },
            AdhocProhibition::Morpheme {
                guid: PROHIBITION_DISABLED.into(),
                disabled: true,
                primary: MSA_ROOT.into(),
                others: Vec::new(),
                adjacency: Adjacency::Anywhere,
            },
        ],
        ..Morphology::default()
    };
    morphology.parser_parameters.active_parser = pg_snapshot::ActiveParser::Hc;

    Snapshot::new(
        Project {
            name: "Facts fixture".into(),
            vernacular_writing_systems: vec!["qaa".into()],
            analysis_writing_systems: vec!["en".into()],
            exemplar_characters: Vec::new(),
        },
        FeatureSystems::default(),
        Phonology {
            phonemes: ["k", "u", "m", "a", "s", "t"]
                .into_iter()
                .enumerate()
                .map(|(i, rep)| Phoneme {
                    guid: format!("00000000-0000-0000-0000-0000000001{:02}", i + 1),
                    name: rep.into(),
                    representations: vec![ws("qaa", rep)],
                    features: None,
                    basic_ipa_symbol: None,
                })
                .collect(),
            boundary_markers: vec![BoundaryMarker {
                guid: "00000000-0000-0000-0000-000000000109".into(),
                name: "Morpheme boundary".into(),
                representations: vec![ws("qaa", "+")],
            }],
            ..Phonology::default()
        },
        morphology,
        Lexicon {
            entries: vec![
                root_entry,
                entry(
                    ENTRY_ROOT_TWO,
                    SENSE_ROOT_TWO,
                    allomorph(ALLO_ROOT_TWO, MorphType::Root, "suma"),
                    root_two_msa,
                    "root two",
                ),
                suffix_entry,
            ],
        },
    )
}

fn context() -> &'static [u8] {
    br#"{"format":"pangloss-facts-context","version":1,"baselineToken":{"id":"baseline-1"},"inputKind":"baseline","dryRunDigest":null}"#
}

fn producer() -> ProducerIdentity<'static> {
    ProducerIdentity {
        compiler_version: "0.6.1",
        source_revision: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        build_identity: "pangloss/0.6.1+aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    }
}

fn build(path: &Path, source: &[u8], context: &[u8]) -> Result<pg_facts::FactsResult, FactsError> {
    pg_facts::write_facts(source, context, path, producer())
}

#[test]
fn exports_authored_msa_references_flat_prohibitions_and_load_outcomes() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("grammar-facts.sqlite");
    let source = snapshot().to_json().into_bytes();

    let result = build(&output, &source, context()).unwrap();
    assert_eq!(result.compile_status, "completed");
    assert_eq!(result.application_id, 1_346_848_321);
    assert_eq!(result.schema_version, 7);
    assert!(result.output_bytes > 0);
    assert_eq!(result.output_sha256.len(), 71);
    assert_eq!(result.source_sha256, pg_assess::source_sha256(&source));
    assert_eq!(result.grammar_hash, snapshot().grammar_hash());

    let db = Connection::open(output).unwrap();
    let application_id: i64 = db
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .unwrap();
    let user_version: i64 = db
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    let encoding: String = db
        .query_row("PRAGMA encoding", [], |row| row.get(0))
        .unwrap();
    let complete: i64 = db
        .query_row(
            "SELECT complete FROM artifact_meta WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(application_id, 1_346_848_321);
    assert_eq!(user_version, 7);
    assert_eq!(encoding, "UTF-8");
    assert_eq!(complete, 1);

    let compiled_allomorph: (String, String) = db
        .query_row(
            "SELECT output_key, identity_quality FROM compiled_mapping \
             WHERE source_kind='allomorph' AND source_guid=?1 AND output_kind='allomorph'",
            [ALLO_ROOT],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(compiled_allomorph.0.contains("#allo0"));
    assert_eq!(compiled_allomorph.1, "structural");
    let compiled_order: (String, i64, i64) = db
        .query_row(
            "SELECT bucket, compiled_order, is_final_elsewhere_case \
             FROM compiled_allomorph_order WHERE source_entry_guid=?1 AND source_msa_guid=?2 \
             AND source_allomorph_guid=?3 AND bucket='Morphology'",
            [ENTRY_ROOT, MSA_ROOT, ALLO_ROOT],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(compiled_order, ("Morphology".into(), 0, 1));
    assert_eq!(
        db.query_row(
            "SELECT status FROM artifact_section WHERE section='compiled_mappings'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "complete"
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM parser_config", [], |row| row
            .get::<_, i64>(0),)
            .unwrap(),
        4
    );

    let (msa_kind, entry_guid): (String, String) = db
        .query_row(
            "SELECT kind, entry_guid FROM msa WHERE msa_guid=?1",
            [MSA_SUFFIX],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(msa_kind, "inflectional");
    assert_eq!(entry_guid, ENTRY_SUFFIX);
    let citation_form: (String, String) = db
        .query_row(
            "SELECT writing_system, form FROM entry_citation_form WHERE entry_guid=?1 AND ordinal=0",
            [ENTRY_ROOT],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(citation_form, ("qaa".into(), "kuma".into()));

    let slot_guid: String = db
        .query_row(
            "SELECT slot_guid FROM msa_slot WHERE msa_guid=?1 AND ordinal=0",
            [MSA_SUFFIX],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(slot_guid, SLOT);
    let unslotted_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM msa_slot WHERE msa_guid=?1",
            [MSA_UNSLOTTED],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unslotted_count, 0);
    let unslotted_loaded: i64 = db
        .query_row(
            "SELECT loaded FROM load_fact WHERE subject_guid=?1 \
             AND pipeline_stage='compile' AND reason_code='represented' LIMIT 1",
            [MSA_UNSLOTTED],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unslotted_loaded, 1);
    let pos_guid: String = db
        .query_row(
            "SELECT category_guid FROM msa_category WHERE msa_guid=?1 AND role='pos'",
            [MSA_SUFFIX],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pos_guid, POS);

    let others = db
        .prepare("SELECT target_guid FROM adhoc_other WHERE prohibition_guid=?1 ORDER BY ordinal")
        .unwrap()
        .query_map([PROHIBITION_MORPHEME], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        others,
        vec![MSA_ROOT_TWO.to_string(), MSA_SUFFIX.to_string()]
    );
    let allomorph_others = db
        .prepare("SELECT target_guid FROM adhoc_other WHERE prohibition_guid=?1 ORDER BY ordinal")
        .unwrap()
        .query_map([PROHIBITION_ALLOMORPH], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        allomorph_others,
        vec![ALLO_ROOT_TWO.to_string(), ALLO_SUFFIX.to_string()]
    );

    let disabled: i64 = db
        .query_row(
            "SELECT disabled FROM adhoc_prohibition WHERE prohibition_guid=?1",
            [PROHIBITION_DISABLED],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(disabled, 1);

    let loaded: i64 = db
        .query_row(
            "SELECT loaded FROM load_fact WHERE subject_guid=?1 AND reason_code='represented' LIMIT 1",
            [PROHIBITION_MORPHEME],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(loaded, 1);
    let (disabled_disposition, disabled_loaded, disabled_reason): (String, Option<i64>, String) =
        db.query_row(
            "SELECT disposition, loaded, reason_code FROM load_fact WHERE subject_guid=?1",
            [PROHIBITION_DISABLED],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(disabled_disposition, "not_considered");
    assert_eq!(disabled_loaded, None);
    assert_eq!(disabled_reason, "disabled");
    let enabled_loaded: i64 = db
        .query_row(
            "SELECT loaded FROM load_fact WHERE subject_guid=?1 AND reason_code='represented'",
            [PROHIBITION_ALLOMORPH],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(enabled_loaded, 1);

    let template_section = result
        .sections
        .iter()
        .find(|section| section.section == "templates")
        .unwrap();
    assert_eq!(template_section.status, "complete");
    assert_eq!(
        db.query_row(
            "SELECT status FROM artifact_section WHERE section='adhoc_groups'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "unavailable"
    );
    assert_eq!(
        db.query_row(
            "SELECT status FROM artifact_section WHERE section='stats'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "not_requested"
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM stats_run", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0,
        "static facts contain no run rows without the paired stats flags"
    );
    assert_eq!(
        db.query_row(
            "SELECT support FROM stats_counter_support WHERE object_kind='phon_rule' AND counter='uses' AND direction='both'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "not_wired",
        "the support catalog is available even when run stats were not requested"
    );
}

#[test]
fn exports_source_occurrences_only_when_import_provenance_can_establish_them() {
    let temp = tempfile::tempdir().unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../pg-fwdata/tests/data/fixture.fwdata");
    let source = std::fs::read_to_string(&fixture).unwrap();
    let injected = "<rt class=\"LexEntry\"/><rt class=\"LexEntry\" guid=\"00000000-0000-0000-0000-000000000050\"/><rt class=\"ZzUnknown\" guid=\"00000000-0000-0000-0000-0000000000zz\"></rt>";
    let variant = source.replacen(
        "</languageproject>",
        &format!("{injected}</languageproject>"),
        1,
    );
    let variant_path = temp.path().join("source.fwdata");
    std::fs::write(&variant_path, variant).unwrap();
    let (imported, _) = pg_fwdata::import_file(&variant_path).unwrap();
    let expected_occurrences = imported
        .conversion_provenance
        .source_census
        .total_occurrences as i64;
    let imported_path = temp.path().join("imported.sqlite");
    build(&imported_path, imported.to_json().as_bytes(), context()).unwrap();
    let imported_db = Connection::open(imported_path).unwrap();
    let (source_status, census_status, source_count, import_facts, load_status): (
        String,
        String,
        i64,
        i64,
        String,
    ) = imported_db
        .query_row(
            "SELECT m.source_inventory_status, \
                    (SELECT status FROM artifact_section WHERE section='source_census'), \
                    (SELECT COUNT(*) FROM source_object), \
                    (SELECT COUNT(*) FROM load_fact WHERE pipeline_stage='import'), \
                    (SELECT status FROM artifact_section WHERE section='load_accounting') \
             FROM artifact_meta m WHERE singleton=1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(source_status, "importedWithFatalIssues");
    assert_eq!(census_status, "complete");
    assert_eq!(source_count, expected_occurrences);
    assert!(import_facts > 0);
    assert_eq!(load_status, "partial");
    let source_occurrence_reasons: i64 = imported_db
        .query_row(
            "SELECT COUNT(*) FROM load_fact WHERE subject_kind='sourceObject' \
             AND pipeline_stage='import' AND reason_code IN ('missing_guid', 'duplicate_header')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source_occurrence_reasons, 2);
    let unknown_class_reason: i64 = imported_db
        .query_row(
            "SELECT COUNT(*) FROM load_fact WHERE subject_kind='sourceObject' \
             AND pipeline_stage='import' AND disposition='not_considered' \
             AND reason_code='unknown_class'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unknown_class_reason, 1);
    let unknown_class_count: i64 = imported_db
        .query_row(
            "SELECT COUNT(*) FROM source_object \
             WHERE class_name='ZzUnknown' AND handled=0 AND retained=0",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unknown_class_count, 1);

    let mut synthetic = snapshot();
    let synthetic_path = temp.path().join("synthetic.sqlite");
    build(&synthetic_path, synthetic.to_json().as_bytes(), context()).unwrap();
    let synthetic_db = Connection::open(synthetic_path).unwrap();
    let (status, section, rows): (String, String, i64) = synthetic_db
        .query_row(
            "SELECT m.source_inventory_status, \
                    (SELECT status FROM artifact_section WHERE section='source_census'), \
                    (SELECT COUNT(*) FROM source_object) \
             FROM artifact_meta m WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(status, "synthetic");
    assert_eq!(section, "unavailable");
    assert_eq!(rows, 0);

    synthetic.conversion_provenance = Default::default();
    let unknown_path = temp.path().join("unknown.sqlite");
    build(&unknown_path, synthetic.to_json().as_bytes(), context()).unwrap();
    let unknown_db = Connection::open(unknown_path).unwrap();
    let (status, section, rows): (String, String, i64) = unknown_db
        .query_row(
            "SELECT m.source_inventory_status, \
                    (SELECT status FROM artifact_section WHERE section='source_census'), \
                    (SELECT COUNT(*) FROM source_object) \
             FROM artifact_meta m WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(status, "unknown");
    assert_eq!(section, "unavailable");
    assert_eq!(rows, 0);
}

#[test]
fn exports_adhoc_signatures_group_rationale_and_loader_outcomes() {
    const PROHIBITION_DUPLICATE: &str = "00000000-0000-0000-0000-000000000043";
    const PROHIBITION_FIRST_TARGET: &str = "00000000-0000-0000-0000-000000000044";
    const PROHIBITION_SECOND_TARGET: &str = "00000000-0000-0000-0000-000000000045";
    const PROHIBITION_EMPTY: &str = "00000000-0000-0000-0000-000000000046";
    const PROHIBITION_UNRESOLVED: &str = "00000000-0000-0000-0000-000000000047";
    const PROHIBITION_WRONG_KIND: &str = "00000000-0000-0000-0000-000000000048";
    const GROUP: &str = "00000000-0000-0000-0000-000000000060";

    let mut authored = snapshot();
    authored.morphology.parts_of_speech[0]
        .affix_slots
        .push(AffixSlot {
            guid: SLOT_EMPTY.into(),
            name: "Unreachable".into(),
            optional: false,
        });
    authored.lexicon.entries.push(entry(
        ENTRY_ORPHAN,
        SENSE_ORPHAN,
        allomorph(ALLO_ORPHAN, MorphType::Suffix, "ka"),
        Msa::Inflectional {
            guid: MSA_ORPHAN.into(),
            part_of_speech: Some(POS.into()),
            slots: vec![SLOT_EMPTY.into()],
            features: None,
            exception_features: Vec::new(),
        },
        "orphan affix",
    ));
    authored
        .morphology
        .adhoc_prohibitions
        .push(AdhocProhibition::Allomorph {
            guid: PROHIBITION_COMPACTED_TARGET.into(),
            disabled: false,
            primary: ALLO_SUFFIX.into(),
            others: vec![ALLO_ORPHAN.into()],
            adjacency: Adjacency::Anywhere,
        });
    let mut source = serde_json::to_value(authored).unwrap();
    let rules = source["morphology"]["adhocProhibitions"]
        .as_array_mut()
        .unwrap();
    let original = rules[0].clone();
    let mut duplicate = original.clone();
    duplicate["guid"] = PROHIBITION_DUPLICATE.into();
    rules.push(duplicate);
    let mut first_target = original.clone();
    first_target["guid"] = PROHIBITION_FIRST_TARGET.into();
    first_target["others"] = serde_json::json!([MSA_ROOT_TWO]);
    rules.push(first_target);
    let mut second_target = original.clone();
    second_target["guid"] = PROHIBITION_SECOND_TARGET.into();
    second_target["others"] = serde_json::json!([MSA_SUFFIX]);
    rules.push(second_target);
    rules.push(serde_json::json!({
        "kind": "morpheme",
        "guid": PROHIBITION_EMPTY,
        "disabled": false,
        "primary": MSA_ROOT,
        "others": [],
        "adjacency": "anywhere"
    }));
    rules.push(serde_json::json!({
        "kind": "morpheme",
        "guid": PROHIBITION_UNRESOLVED,
        "disabled": false,
        "primary": "00000000-0000-0000-0000-000000000049",
        "others": [MSA_SUFFIX],
        "adjacency": "anywhere"
    }));
    rules.push(serde_json::json!({
        "kind": "allomorph",
        "guid": PROHIBITION_WRONG_KIND,
        "disabled": false,
        "primary": MSA_ROOT,
        "others": [ALLO_SUFFIX],
        "adjacency": "anywhere"
    }));
    source["morphology"]["adhocProhibitionGroups"] = serde_json::json!([{
        "guid": GROUP,
        "name": [
            {"ws": "en", "form": "Grouped restriction"},
            {"ws": "qaa", "form": "Restriction rationale"}
        ],
        "description": [
            {"ws": "en", "form": "These rules express a missing generalization."},
            {"ws": "qaa", "form": "The authored explanation."}
        ],
        "members": [PROHIBITION_ALLOMORPH, PROHIBITION_MORPHEME]
    }]);
    let source = serde_json::to_vec(&source).unwrap();

    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("grammar-facts.sqlite");
    build(&output, &source, context()).unwrap();
    let db = Connection::open(output).unwrap();

    let mut signatures = db
        .prepare(
            "SELECT p.kind, p.primary_guid, \
             (SELECT json_group_array(target_guid) FROM (SELECT target_guid FROM adhoc_other o \
              WHERE o.prohibition_guid=p.prohibition_guid ORDER BY ordinal)), \
             p.adjacency, p.disabled=0, f.disposition, f.loaded, f.reason_code, f.pipeline_stage \
             FROM adhoc_prohibition p LEFT JOIN load_fact f \
             ON f.subject_guid=p.prohibition_guid \
             AND f.subject_kind=CASE p.kind WHEN 'allomorph' THEN 'allomorphCoOccurrence' \
                  ELSE 'morphemeCoOccurrence' END \
             AND f.pipeline_stage=CASE WHEN EXISTS (SELECT 1 FROM load_fact c \
                  WHERE c.subject_guid=p.prohibition_guid AND c.subject_kind=f.subject_kind \
                    AND c.pipeline_stage='compact') THEN 'compact' ELSE 'compile' END \
             WHERE p.prohibition_guid=?1",
        )
        .unwrap();
    let mut signature = |guid: &str| {
        signatures
            .query_row([guid], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            })
            .unwrap()
    };
    let original_signature = signature(PROHIBITION_MORPHEME);
    let duplicate_signature = signature(PROHIBITION_DUPLICATE);
    assert_eq!(
        (
            &original_signature.0,
            &original_signature.1,
            &original_signature.2,
            &original_signature.3,
        ),
        (
            &duplicate_signature.0,
            &duplicate_signature.1,
            &duplicate_signature.2,
            &duplicate_signature.3,
        ),
        "the two rules must retain the same ordered signature"
    );
    assert_eq!(original_signature.0, "morpheme");
    assert_eq!(original_signature.1, MSA_ROOT);
    assert_eq!(
        original_signature.2,
        format!("[\"{MSA_ROOT_TWO}\",\"{MSA_SUFFIX}\"]")
    );
    assert_eq!(original_signature.3, "somewhereToRight");
    assert_eq!(original_signature.4, 1);
    assert_eq!(
        signature(PROHIBITION_FIRST_TARGET).2,
        format!("[\"{MSA_ROOT_TWO}\"]")
    );
    assert_eq!(
        signature(PROHIBITION_SECOND_TARGET).2,
        format!("[\"{MSA_SUFFIX}\"]")
    );
    assert_eq!(original_signature.5.as_deref(), Some("represented"));
    assert_eq!(original_signature.6, Some(1));
    assert_eq!(original_signature.8.as_deref(), Some("compile"));

    let empty = signature(PROHIBITION_EMPTY);
    assert_eq!(empty.5.as_deref(), Some("rejected"));
    assert_eq!(empty.6, Some(0));
    let unresolved = signature(PROHIBITION_UNRESOLVED);
    assert_eq!(unresolved.5.as_deref(), Some("rejected"));
    let wrong_kind = signature(PROHIBITION_WRONG_KIND);
    assert_eq!(wrong_kind.0, "allomorph");
    assert_eq!(
        wrong_kind.1, MSA_ROOT,
        "the source MSA GUID must not be relabeled"
    );
    assert_eq!(wrong_kind.5.as_deref(), Some("rejected"));
    let disabled = signature(PROHIBITION_DISABLED);
    assert_eq!(disabled.4, 0);
    assert_eq!(disabled.5.as_deref(), Some("not_considered"));
    assert_eq!(disabled.6, None);
    assert_eq!(disabled.7.as_deref(), Some("disabled"));
    assert_eq!(disabled.8.as_deref(), Some("compile"));
    let compacted_rule = signature(PROHIBITION_COMPACTED_TARGET);
    assert_eq!(compacted_rule.5.as_deref(), Some("compacted"));
    assert_eq!(compacted_rule.6, Some(0));
    assert_eq!(compacted_rule.8.as_deref(), Some("compact"));
    let compacted: (String, String, Option<i64>, String) = db
        .query_row(
            "SELECT pipeline_stage, disposition, loaded, reason_code FROM load_fact \
             WHERE subject_guid=?1 AND pipeline_stage='compact'",
            [PROHIBITION_COMPACTED_TARGET],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(compacted.0, "compact");
    assert_eq!(compacted.1, "compacted");
    assert_eq!(compacted.2, Some(0));
    let compacted_target: String = db
        .query_row(
            "SELECT inventory_stage FROM conversion_item WHERE subject_guid=?1 \
             AND pipeline_stage='compact'",
            [ALLO_ORPHAN],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(compacted_target, "rejected");

    let group_text = db
        .prepare(
            "SELECT field, writing_system, text FROM adhoc_group_text \
             WHERE group_guid=?1 ORDER BY field, writing_system",
        )
        .unwrap()
        .query_map([GROUP], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(group_text.len(), 4);
    assert!(group_text.contains(&(
        "description".into(),
        "en".into(),
        "These rules express a missing generalization.".into()
    )));
    assert!(group_text.contains(&(
        "description".into(),
        "qaa".into(),
        "The authored explanation.".into()
    )));
    assert!(group_text.contains(&("name".into(), "en".into(), "Grouped restriction".into())));
    assert!(group_text.contains(&("name".into(), "qaa".into(), "Restriction rationale".into())));
    let members = db
        .prepare(
            "SELECT member_guid FROM adhoc_group_member WHERE group_guid=?1 ORDER BY member_guid",
        )
        .unwrap()
        .query_map([GROUP], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        members,
        vec![
            PROHIBITION_MORPHEME.to_string(),
            PROHIBITION_ALLOMORPH.to_string()
        ]
    );
    let group_section: String = db
        .query_row(
            "SELECT status FROM artifact_section WHERE section='adhoc_groups'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(group_section, "complete");

    let mut reordered: serde_json::Value = serde_json::from_slice(&source).unwrap();
    reordered["morphology"]["adhocProhibitionGroups"][0]["members"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let reordered_source = serde_json::to_vec(&reordered).unwrap();
    let reordered_output = temp.path().join("reordered-facts.sqlite");
    build(&reordered_output, &reordered_source, context()).unwrap();
    let reordered_db = Connection::open(reordered_output).unwrap();
    let reordered_members = reordered_db
        .prepare(
            "SELECT member_guid FROM adhoc_group_member WHERE group_guid=?1 ORDER BY member_guid",
        )
        .unwrap()
        .query_map([GROUP], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(reordered_members, members);
}

#[test]
fn projects_template_order_allomorph_order_and_every_form_writing_system() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("grammar-facts.sqlite");
    let mut source = snapshot();
    let pos = &mut source.morphology.parts_of_speech[0];
    pos.affix_slots.extend([
        AffixSlot {
            guid: SLOT_TWO.into(),
            name: "Number".into(),
            optional: true,
        },
        AffixSlot {
            guid: SLOT_THREE.into(),
            name: "Case".into(),
            optional: false,
        },
    ]);
    pos.affix_templates[0].suffix_slots = vec![SLOT.into(), SLOT_TWO.into()];
    pos.affix_templates[0].prefix_slots = vec![SLOT_THREE.into(), SLOT.into()];
    let suffix = source
        .lexicon
        .entries
        .iter_mut()
        .find(|entry| entry.guid == ENTRY_SUFFIX)
        .unwrap();
    if let Msa::Inflectional { slots, .. } = &mut suffix.msas[0] {
        *slots = vec![SLOT.into(), SLOT_TWO.into(), SLOT_THREE.into()];
    } else {
        panic!("suffix fixture must have an inflectional MSA");
    }
    let root = source
        .lexicon
        .entries
        .iter_mut()
        .find(|entry| entry.guid == ENTRY_ROOT)
        .unwrap();
    root.citation_form.push(ws("en", "Kuma"));
    let mut alternate = allomorph(ALLO_ROOT_ALT, MorphType::Root, "suma");
    alternate.forms.push(ws("en", "alternate root"));
    let mut abstract_form = allomorph(ALLO_ROOT_ABSTRACT, MorphType::Root, "kuma");
    abstract_form.is_abstract = true;
    let empty_form = allomorph(ALLO_ROOT_EMPTY, MorphType::Root, "");
    root.allomorphs
        .splice(0..0, [alternate, abstract_form, empty_form]);
    root.allomorphs[3].forms.push(ws("en", "root in English"));

    let source_bytes = serde_json::to_vec(&source).unwrap();
    build(&output, &source_bytes, context()).unwrap();
    let db = Connection::open(output).unwrap();

    let template_slots = {
        let mut statement = db
            .prepare(
                "SELECT side, ordinal, slot_guid, compiled_order FROM template_slot \
                 WHERE template_guid=?1 ORDER BY side, ordinal",
            )
            .unwrap();
        statement
            .query_map([TEMPLATE], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        template_slots,
        vec![
            ("prefix".into(), 0, SLOT_THREE.into(), Some(3)),
            ("prefix".into(), 1, SLOT.into(), Some(2)),
            ("suffix".into(), 0, SLOT.into(), Some(0)),
            ("suffix".into(), 1, SLOT_TWO.into(), Some(1)),
        ]
    );

    let root_order = {
        let mut statement = db
            .prepare("SELECT guid FROM allomorph WHERE entry_guid=?1 ORDER BY ordinal")
            .unwrap();
        statement
            .query_map([ENTRY_ROOT], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        root_order,
        vec![
            ALLO_ROOT_ALT,
            ALLO_ROOT_ABSTRACT,
            ALLO_ROOT_EMPTY,
            ALLO_ROOT
        ]
    );
    let filtered_root_order = {
        let mut statement = db
            .prepare(
                "SELECT source_allomorph_guid, compiled_order, is_final_elsewhere_case \
                 FROM compiled_allomorph_order WHERE source_entry_guid=?1 AND source_msa_guid=?2 \
                   AND bucket='Morphology' AND source_allomorph_guid IN (?3, ?4) \
                 ORDER BY source_allomorph_guid",
            )
            .unwrap();
        statement
            .query_map(
                [ENTRY_ROOT, MSA_ROOT, ALLO_ROOT_ABSTRACT, ALLO_ROOT_EMPTY],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        filtered_root_order,
        vec![
            (Some(ALLO_ROOT_ABSTRACT.into()), None, 0),
            (Some(ALLO_ROOT_EMPTY.into()), None, 0),
        ],
        "compiler-filtered forms retain source identity without invented final order"
    );
    let suffix_order = {
        let mut statement = db
            .prepare(
                "SELECT source_entry_guid, source_msa_guid, bucket, source_allomorph_guid, \
                 compiled_order, is_final_elsewhere_case FROM compiled_allomorph_order \
                 WHERE source_entry_guid=?1 AND source_msa_guid=?2 AND bucket='Morphology' \
                 ORDER BY compiled_order, source_allomorph_guid",
            )
            .unwrap();
        statement
            .query_map([ENTRY_SUFFIX, MSA_SUFFIX], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        suffix_order,
        vec![(
            Some(ENTRY_SUFFIX.into()),
            Some(MSA_SUFFIX.into()),
            "Morphology".into(),
            Some(ALLO_SUFFIX.into()),
            Some(0),
            1,
        )]
    );
    let abstract_flag: i64 = db
        .query_row(
            "SELECT is_abstract FROM allomorph WHERE guid=?1",
            [ALLO_ROOT_ABSTRACT],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(abstract_flag, 1);
    let retained_empty_form: String = db
        .query_row(
            "SELECT form FROM allomorph_form WHERE allomorph_guid=?1 AND ordinal=0",
            [ALLO_ROOT_EMPTY],
            |row| row.get(0),
        )
        .unwrap();
    assert!(retained_empty_form.is_empty());
    let citation_forms = {
        let mut statement = db
            .prepare(
                "SELECT writing_system, form FROM entry_citation_form \
                 WHERE entry_guid=?1 ORDER BY ordinal",
            )
            .unwrap();
        statement
            .query_map([ENTRY_ROOT], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        citation_forms,
        vec![("qaa".into(), "kuma".into()), ("en".into(), "Kuma".into())]
    );
    let root_forms = {
        let mut statement = db
            .prepare(
                "SELECT writing_system, form FROM allomorph_form \
                 WHERE allomorph_guid=?1 ORDER BY ordinal",
            )
            .unwrap();
        statement
            .query_map([ALLO_ROOT_ALT], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        root_forms,
        vec![
            ("qaa".into(), "suma".into()),
            ("en".into(), "alternate root".into())
        ]
    );
    let root_msa_slots: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM msa_slot WHERE msa_guid=?1 AND role='slot'",
            [MSA_SUFFIX],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(root_msa_slots, 3);
    for (guid, reason) in [
        (ALLO_ROOT_ABSTRACT, "abstract"),
        (ALLO_ROOT_EMPTY, "emptyForm"),
    ] {
        let recorded_reason: String = db
            .query_row(
                "SELECT reason_code FROM load_fact \
                 WHERE subject_kind='allomorph' AND subject_guid=?1 \
                   AND pipeline_stage='compile' AND context_key='lexEntryForm:morphology'",
                [guid],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(recorded_reason, reason);
    }
}

#[test]
fn retains_disabled_empty_and_pruned_templates_with_owner_reasons() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("grammar-facts.sqlite");
    let mut source = snapshot();
    let pos = &mut source.morphology.parts_of_speech[0];
    pos.affix_slots.push(AffixSlot {
        guid: SLOT_EMPTY.into(),
        name: "Unused".into(),
        optional: false,
    });
    pos.affix_templates.extend([
        AffixTemplate {
            guid: TEMPLATE_DISABLED.into(),
            name: "Disabled template".into(),
            disabled: true,
            prefix_slots: Vec::new(),
            suffix_slots: vec![SLOT.into()],
            is_final: false,
        },
        AffixTemplate {
            guid: TEMPLATE_EMPTY.into(),
            name: "Empty template".into(),
            disabled: false,
            prefix_slots: Vec::new(),
            suffix_slots: Vec::new(),
            is_final: false,
        },
        AffixTemplate {
            guid: TEMPLATE_NO_RULES.into(),
            name: "Unused slot template".into(),
            disabled: false,
            prefix_slots: Vec::new(),
            suffix_slots: vec![SLOT_EMPTY.into(), SLOT.into()],
            is_final: false,
        },
        AffixTemplate {
            guid: TEMPLATE_SHARED.into(),
            name: "Shared slot template".into(),
            disabled: false,
            prefix_slots: Vec::new(),
            suffix_slots: vec![SLOT.into()],
            is_final: false,
        },
    ]);
    build(&output, &serde_json::to_vec(&source).unwrap(), context()).unwrap();
    let db = Connection::open(output).unwrap();

    let (disabled, final_template): (i64, i64) = db
        .query_row(
            "SELECT disabled, is_final FROM affix_template WHERE guid=?1",
            [TEMPLATE_DISABLED],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((disabled, final_template), (1, 0));
    let disabled_slot_order: Option<i64> = db
        .query_row(
            "SELECT compiled_order FROM template_slot \
             WHERE template_guid=?1 AND side='suffix' AND ordinal=0",
            [TEMPLATE_DISABLED],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(disabled_slot_order, None);
    let empty_template_reason: String = db
        .query_row(
            "SELECT reason_code FROM load_fact \
             WHERE subject_kind='template' AND subject_guid=?1 \
               AND pipeline_stage='compile'",
            [TEMPLATE_EMPTY],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(empty_template_reason, "grammar.template.no-slots");
    let disabled_template_reason: (String, Option<i64>) = db
        .query_row(
            "SELECT reason_code, loaded FROM load_fact \
             WHERE subject_kind='template' AND subject_guid=?1 \
               AND pipeline_stage='compile'",
            [TEMPLATE_DISABLED],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(disabled_template_reason, ("disabled".into(), None));
    let dropped_slot: Option<i64> = db
        .query_row(
            "SELECT compiled_order FROM template_slot \
             WHERE template_guid=?1 AND side='suffix' AND ordinal=0",
            [TEMPLATE_NO_RULES],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(dropped_slot, None);
    let shifted_slot: Option<i64> = db
        .query_row(
            "SELECT compiled_order FROM template_slot \
             WHERE template_guid=?1 AND side='suffix' AND ordinal=1",
            [TEMPLATE_NO_RULES],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(shifted_slot, Some(0));
    let dropped_slot_reason: String = db
        .query_row(
            "SELECT reason_code FROM load_fact WHERE subject_guid=?1 \
             AND pipeline_stage='compile' AND reason_code='grammar.template.slot-no-rules' \
             LIMIT 1",
            [SLOT_EMPTY],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(dropped_slot_reason, "grammar.template.slot-no-rules");
    let shared_orders = db
        .prepare(
            "SELECT template_guid, compiled_order FROM template_slot \
             WHERE slot_guid=?1 AND compiled_order IS NOT NULL ORDER BY template_guid",
        )
        .unwrap()
        .query_map([SLOT], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        shared_orders,
        vec![
            (TEMPLATE.to_string(), 0),
            (TEMPLATE_NO_RULES.to_string(), 0),
            (TEMPLATE_SHARED.to_string(), 0)
        ]
    );
    let slot_drop_reason_count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM load_fact \
             WHERE pipeline_stage='compile' AND reason_code='grammar.template.slot-no-rules'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(slot_drop_reason_count >= 1);
    let status: String = db
        .query_row(
            "SELECT status FROM artifact_section WHERE section='templates'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "complete");
}

#[test]
fn dangling_template_slot_and_category_references_remain_authored() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("grammar-facts.sqlite");
    let mut source = snapshot();
    let missing_slot = "00000000-0000-0000-0000-000000000099";
    let missing_category = "00000000-0000-0000-0000-000000000098";
    source.morphology.parts_of_speech[0].affix_templates[0]
        .suffix_slots
        .push(missing_slot.into());
    if let Msa::Inflectional { part_of_speech, .. } = &mut source.lexicon.entries[2].msas[0] {
        *part_of_speech = Some(missing_category.into());
    } else {
        panic!("suffix fixture must have an inflectional MSA");
    }
    build(&output, &serde_json::to_vec(&source).unwrap(), context()).unwrap();
    let db = Connection::open(output).unwrap();

    let authored_slot: String = db
        .query_row(
            "SELECT slot_guid FROM template_slot \
             WHERE template_guid=?1 AND side='suffix' AND ordinal=1",
            [TEMPLATE],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(authored_slot, missing_slot);
    let authored_category: String = db
        .query_row(
            "SELECT category_guid FROM msa_category \
             WHERE msa_guid=?1 AND role='pos' AND ordinal=0",
            [MSA_SUFFIX],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(authored_category, missing_category);
}

#[test]
fn t2_lomi_gold_queries_preserve_slot_order_allomorphs_and_parse_outcomes() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("t2-facts.sqlite");
    let source = t2_snapshot();
    let parses_without_facts: BTreeSet<_> = t2_parse_signatures(&source).into_iter().collect();
    assert!(
        parses_without_facts
            .iter()
            .any(|(word, signature)| word == "kumanaride" && signature != "-"),
        "T2 noun order witness must parse"
    );
    assert!(
        parses_without_facts
            .iter()
            .any(|(word, signature)| word == "agakami" && signature != "-"),
        "T2 verb order witness must parse"
    );
    build(&output, &serde_json::to_vec(&source).unwrap(), context()).unwrap();
    let db = Connection::open(output).unwrap();

    let template_slots = {
        let mut statement = db
            .prepare(
                "SELECT t.guid, s.name, s.optional, ts.side, ts.ordinal, ts.compiled_order \
                 FROM template_slot ts \
                 JOIN affix_template t ON t.guid=ts.template_guid \
                 JOIN affix_slot s ON s.guid=ts.slot_guid \
                 ORDER BY t.guid, ts.ordinal",
            )
            .unwrap();
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        template_slots,
        vec![
            (
                T2_TEMPLATE_NOUN.into(),
                "Noun class".into(),
                0,
                "suffix".into(),
                0,
                Some(0)
            ),
            (
                T2_TEMPLATE_NOUN.into(),
                "Number".into(),
                1,
                "suffix".into(),
                1,
                Some(1)
            ),
            (
                T2_TEMPLATE_NOUN.into(),
                "Case".into(),
                1,
                "suffix".into(),
                2,
                Some(2)
            ),
            (
                T2_TEMPLATE_VERB.into(),
                "Tense".into(),
                1,
                "suffix".into(),
                0,
                Some(0)
            ),
            (
                T2_TEMPLATE_VERB.into(),
                "Person".into(),
                1,
                "suffix".into(),
                1,
                Some(1)
            ),
        ]
    );
    let class_marker_forms = {
        let mut statement = db
            .prepare(
                "SELECT a.guid, f.form FROM allomorph a \
                 JOIN allomorph_form f ON f.allomorph_guid=a.guid \
                 WHERE a.entry_guid=?1 ORDER BY a.ordinal, f.ordinal",
            )
            .unwrap();
        statement
            .query_map([T2_ENTRY_CLASS], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        class_marker_forms,
        vec![
            (T2_ALLO_CLASS_A.into(), "na".into()),
            (T2_ALLO_CLASS_B.into(), "ta".into())
        ]
    );
    let class_marker_gloss: String = db
        .query_row(
            "SELECT text FROM sense_text st \
             JOIN sense s ON s.sense_guid=st.sense_guid \
             WHERE s.entry_guid=?1 AND st.kind='gloss' ORDER BY st.ordinal LIMIT 1",
            [T2_ENTRY_CLASS],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(class_marker_gloss, "NCL");
    let (class_msa_count, class_allomorph_count): (i64, i64) = db
        .query_row(
            "SELECT (SELECT COUNT(*) FROM msa WHERE entry_guid=?1), \
                    (SELECT COUNT(*) FROM allomorph WHERE entry_guid=?1)",
            [T2_ENTRY_CLASS],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((class_msa_count, class_allomorph_count), (1, 2));
    let tense_allomorph_order = {
        let mut statement = db
            .prepare("SELECT guid FROM allomorph WHERE entry_guid=?1 ORDER BY ordinal")
            .unwrap();
        statement
            .query_map([T2_ENTRY_TENSE], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        tense_allomorph_order,
        vec![T2_ALLO_TENSE_VOWEL, T2_ALLO_TENSE_CONSONANT]
    );
    for section in ["templates", "allomorphs"] {
        let status: String = db
            .query_row(
                "SELECT status FROM artifact_section WHERE section=?1",
                [section],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "complete", "{section} section status");
    }

    let parses_with_facts: BTreeSet<_> = t2_parse_signatures(&source).into_iter().collect();
    let facts_only: Vec<_> = parses_with_facts
        .difference(&parses_without_facts)
        .cloned()
        .collect();
    let facts_off_only: Vec<_> = parses_without_facts
        .difference(&parses_with_facts)
        .cloned()
        .collect();
    assert!(
        facts_only.is_empty(),
        "facts-on-only parses: {facts_only:?}"
    );
    assert!(
        facts_off_only.is_empty(),
        "facts-off-only parses: {facts_off_only:?}"
    );
}

#[test]
fn repeated_export_is_byte_deterministic_and_does_not_replace_an_existing_file() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first.sqlite");
    let second = temp.path().join("second.sqlite");
    let source = snapshot().to_json().into_bytes();

    let first_result = build(&first, &source, context()).unwrap();
    let second_result = build(&second, &source, context()).unwrap();
    let first_bytes = std::fs::read(&first).unwrap();
    assert_eq!(first_bytes, std::fs::read(second).unwrap());
    assert_eq!(first_result.output_sha256, second_result.output_sha256);

    let error = build(&first, &source, context()).unwrap_err();
    assert!(matches!(error, FactsError::OutputExists));
    assert_eq!(first_bytes, std::fs::read(&first).unwrap());
}

#[test]
fn malformed_or_mismatched_context_publishes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let source = snapshot().to_json().into_bytes();
    let bad_context = br#"{"format":"pangloss-facts-context","version":1,"baselineToken":{},"inputKind":"proposal-dry-run","dryRunDigest":null}"#;
    let output = temp.path().join("facts.sqlite");
    assert!(matches!(
        build(&output, &source, bad_context),
        Err(FactsError::InvalidContext(_))
    ));
    assert!(!output.exists());

    let mismatch = br#"{"format":"pangloss-facts-context","version":1,"baselineToken":{},"inputKind":"baseline","dryRunDigest":null,"expectedModelFingerprint":"sha256:0000000000000000000000000000000000000000000000000000000000000000"}"#;
    assert!(matches!(
        build(&output, &source, mismatch),
        Err(FactsError::ModelMismatch { .. })
    ));
    assert!(!output.exists());
}

#[test]
fn context_rejects_unknown_properties_and_invalid_dry_run_scope() {
    let temp = tempfile::tempdir().unwrap();
    let source = snapshot().to_json().into_bytes();
    let output = temp.path().join("facts.sqlite");
    let unknown = br#"{"format":"pangloss-facts-context","version":1,"baselineToken":{},"inputKind":"baseline","dryRunDigest":null,"extra":true}"#;
    assert!(matches!(
        build(&output, &source, unknown),
        Err(FactsError::InvalidContext(_))
    ));
    let dry_run_without_digest = br#"{"format":"pangloss-facts-context","version":1,"baselineToken":{},"inputKind":"proposal-dry-run","dryRunDigest":null}"#;
    assert!(matches!(
        build(&output, &source, dry_run_without_digest),
        Err(FactsError::InvalidContext(_))
    ));
    assert!(!output.exists());
}

#[test]
fn proposal_dry_run_identity_is_preserved_in_the_result_and_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let source = snapshot().to_json().into_bytes();
    let output = temp.path().join("proposal-facts.sqlite");
    let context = br#"{
        "format":"pangloss-facts-context",
        "version":1,
        "baselineToken":{"id":"baseline-1"},
        "inputKind":"proposal-dry-run",
        "dryRunDigest":"sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    }"#;

    let result = build(&output, &source, context).unwrap();
    assert_eq!(result.input_kind, "proposal-dry-run");
    assert_eq!(
        result.dry_run_digest.as_deref(),
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    let db = Connection::open(output).unwrap();
    let (input_kind, dry_run_digest): (String, String) = db
        .query_row(
            "SELECT input_kind, dry_run_digest FROM artifact_meta WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(input_kind, "proposal-dry-run");
    assert_eq!(
        dry_run_digest,
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
}

#[test]
fn unsupported_context_and_snapshot_versions_publish_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let source = snapshot().to_json().into_bytes();
    let output = temp.path().join("facts.sqlite");
    let context_v2 = br#"{"format":"pangloss-facts-context","version":2,"baselineToken":{},"inputKind":"baseline","dryRunDigest":null}"#;
    assert!(matches!(
        build(&output, &source, context_v2),
        Err(FactsError::UnsupportedContextVersion(2))
    ));
    let mut snapshot_v2: serde_json::Value = serde_json::from_slice(&source).unwrap();
    snapshot_v2["version"] = serde_json::json!(2);
    let snapshot_v2 = serde_json::to_vec(&snapshot_v2).unwrap();
    assert!(matches!(
        build(&output, &snapshot_v2, context()),
        Err(FactsError::UnsupportedSnapshotVersion(2))
    ));
    assert!(!output.exists());
}

#[test]
fn non_snapshot_json_is_rejected_before_publication() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("facts.sqlite");
    let mut wrong_format: serde_json::Value =
        serde_json::from_slice(&snapshot().to_json().into_bytes()).unwrap();
    wrong_format["format"] = serde_json::json!("other");
    let wrong_format = serde_json::to_vec(&wrong_format).unwrap();
    assert!(matches!(
        build(&output, &wrong_format, context()),
        Err(FactsError::UnsupportedFactsSource(_))
    ));
    assert!(!output.exists());
}

#[test]
fn structured_compile_refusal_publishes_authored_facts_and_marks_effective_sections_unavailable() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("refused.sqlite");
    let mut input = snapshot();
    input.conversion_provenance = Default::default();

    let result = build(&output, input.to_json().as_bytes(), context()).unwrap();
    assert_eq!(result.compile_status, "refused");
    let db = Connection::open(output).unwrap();
    let (compile_status, complete): (String, i64) = db
        .query_row(
            "SELECT compile_status, complete FROM artifact_meta WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(compile_status, "refused");
    assert_eq!(complete, 1);
    assert_eq!(
        db.query_row(
            "SELECT status FROM artifact_section WHERE section='effective_grammar'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "unavailable"
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM lex_entry", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        3
    );
}

#[test]
fn exports_environment_resolution_feature_extensions_and_rewrite_patterns() {
    const PHONEME_K: &str = "00000000-0000-0000-0000-000000000101";
    const PHONEME_M: &str = "00000000-0000-0000-0000-000000000103";
    const PHONEME_S: &str = "00000000-0000-0000-0000-000000000105";
    const PHONEME_T: &str = "00000000-0000-0000-0000-000000000106";
    const PHONEME_TS: &str = "00000000-0000-0000-0000-000000000110";
    const BOUNDARY_PLUS: &str = "00000000-0000-0000-0000-000000000109";

    let mut authored = snapshot();
    authored.feature_systems.phonological.closed_features = vec![ClosedFeature {
        guid: FEATURE_VOICE.into(),
        name: "Voicing".into(),
        abbreviation: "voi".into(),
        values: vec![
            FeatureValueSymbol {
                guid: VALUE_PLUS.into(),
                name: "Voiced".into(),
                abbreviation: "+".into(),
            },
            FeatureValueSymbol {
                guid: VALUE_MINUS.into(),
                name: "Voiceless".into(),
                abbreviation: "-".into(),
            },
        ],
    }];
    for phoneme in &mut authored.phonology.phonemes {
        let value = if phoneme.guid == PHONEME_K {
            VALUE_PLUS
        } else {
            VALUE_MINUS
        };
        phoneme.features = Some(FeatureStructure {
            values: vec![FeatureValue {
                feature: FEATURE_VOICE.into(),
                value: FeatureValueKind::Closed {
                    value: value.into(),
                },
            }],
        });
    }
    authored.phonology.phonemes.push(Phoneme {
        guid: PHONEME_TS.into(),
        name: "ts".into(),
        representations: vec![ws("qaa", "ts")],
        features: None,
        basic_ipa_symbol: None,
    });
    authored.phonology.natural_classes = vec![
        NaturalClass::Segments {
            guid: NC_C.into(),
            name: "C".into(),
            display_name: Some("First C".into()),
            phonemes: vec![PHONEME_K.into(), PHONEME_M.into()],
        },
        NaturalClass::Segments {
            guid: NC_C_DUP.into(),
            name: "C".into(),
            display_name: Some("Second C".into()),
            phonemes: vec![PHONEME_S.into(), PHONEME_T.into()],
        },
        NaturalClass::Features {
            guid: NC_VOICED.into(),
            name: "V".into(),
            display_name: Some("Voiced".into()),
            features: FeatureStructure {
                values: vec![FeatureValue {
                    feature: FEATURE_VOICE.into(),
                    value: FeatureValueKind::Closed {
                        value: VALUE_PLUS.into(),
                    },
                }],
            },
        },
    ];
    authored.phonology.environments = vec![
        Environment {
            guid: ENV_VALID.into(),
            name: "Anchored".into(),
            representation: "/#_".into(),
        },
        Environment {
            guid: ENV_REPEAT.into(),
            name: "Repeated optional class".into(),
            representation: "/([C][C])_".into(),
        },
        Environment {
            guid: ENV_INVALID.into(),
            name: "Invalid on right".into(),
            representation: "/[C]_[Missing]".into(),
        },
        Environment {
            guid: ENV_UNUSED.into(),
            name: "Unused".into(),
            representation: "/_[C]".into(),
        },
        Environment {
            guid: ENV_RIGHT_ANCHOR.into(),
            name: "Empty left, multigraph and right anchor".into(),
            representation: "/_ts#".into(),
        },
    ];
    authored.lexicon.entries[0].allomorphs[0].environments = vec![ENV_VALID.into()];
    authored.lexicon.entries[1].allomorphs[0].environments = vec![ENV_RIGHT_ANCHOR.into()];
    authored.lexicon.entries[2].allomorphs[0].environments = vec![ENV_VALID.into()];
    authored.lexicon.entries[2].allomorphs[0].positions = vec![ENV_REPEAT.into()];
    authored
        .phonology
        .rules
        .push(PhonologicalRule::Rewrite(RewriteRule {
            guid: RULE_REWRITE.into(),
            name: "k to m before V".into(),
            direction: RuleDirection::LeftToRight,
            structural_description: vec![PhonContext::Segment {
                phoneme: PHONEME_K.into(),
            }],
            feature_constraint_variables: Vec::new(),
            right_hand_sides: vec![RewriteRhs {
                structural_change: vec![PhonContext::Segment {
                    phoneme: PHONEME_M.into(),
                }],
                left_context: None,
                right_context: Some(PhonContext::Sequence {
                    members: vec![
                        PhonContext::NaturalClass {
                            natural_class: NC_VOICED.into(),
                            plus_variables: Vec::new(),
                            minus_variables: Vec::new(),
                        },
                        PhonContext::Boundary {
                            marker: BOUNDARY_PLUS.into(),
                        },
                    ],
                }),
                required_parts_of_speech: Vec::new(),
                required_rule_features: Vec::new(),
                excluded_rule_features: Vec::new(),
            }],
        }));
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("grammar-facts.sqlite");
    let result = build(&output, authored.to_json().as_bytes(), context()).unwrap();
    assert_eq!(result.compile_status, "completed");
    assert_eq!(result.schema_version, 7);
    let db = Connection::open(output).unwrap();

    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        7
    );
    assert_eq!(
        db.query_row(
            "SELECT parse_status FROM environment WHERE guid=?1",
            [ENV_VALID],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "valid"
    );
    assert_eq!(
        db.query_row(
            "SELECT parse_status FROM environment WHERE guid=?1",
            [ENV_UNUSED],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "not_attempted"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM environment_natural_class WHERE environment_guid=?1 AND result='resolved' AND natural_class_guid=?2",
            rusqlite::params![ENV_REPEAT, NC_C],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM environment_natural_class WHERE environment_guid=?1 AND natural_class_guid=?2",
            rusqlite::params![ENV_REPEAT, NC_C_DUP],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0,
        "duplicate abbreviations resolve to the compiler's first-declared winner"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM pattern_node WHERE root_id=(SELECT root_id FROM pattern_root WHERE owner_guid=?1 AND role='environment_left') AND kind='quantifier' AND min=0 AND max=1",
            [ENV_REPEAT],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM natural_class_effective_member WHERE natural_class_guid=?1 AND phoneme_guid=?2",
            rusqlite::params![NC_VOICED, PHONEME_M],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0,
        "the feature extension differs from the explicit segment list that contains M"
    );
    assert_eq!(
        db.query_row(
            "SELECT json_extract(member_key, '$.kind') FROM natural_class_effective_member WHERE natural_class_guid=?1 AND phoneme_guid=?2",
            rusqlite::params![NC_VOICED, PHONEME_K],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "object"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM pattern_root WHERE owner_guid=?1 AND role='environment_right'",
            [ENV_REPEAT],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0,
        "the empty right side is preserved as an absent pattern root"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM pattern_root WHERE owner_guid=?1 AND role='environment_left'",
            [ENV_RIGHT_ANCHOR],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0,
        "the empty left side is preserved as an absent pattern root"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM pattern_node WHERE root_id=(SELECT root_id FROM pattern_root WHERE owner_guid=?1 AND role='environment_left') AND kind='leftAnchor'",
            [ENV_VALID],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM pattern_node WHERE root_id=(SELECT root_id FROM pattern_root WHERE owner_guid=?1 AND role='environment_right') AND kind='rightAnchor'",
            [ENV_RIGHT_ANCHOR],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM pattern_node WHERE root_id=(SELECT root_id FROM pattern_root WHERE owner_guid=?1 AND role='environment_right') AND kind='literalSegments' AND token_text='ts'",
            [ENV_RIGHT_ANCHOR],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM pattern_node AS child JOIN pattern_node AS parent ON parent.node_id=child.parent_node_id WHERE parent.root_id=(SELECT root_id FROM pattern_root WHERE owner_guid=?1 AND role='environment_right') AND parent.token_text='ts' AND child.kind='phoneme' AND child.phoneme_guid=?2",
            rusqlite::params![ENV_RIGHT_ANCHOR, PHONEME_TS],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1,
        "multigraph literal text points to its compiled source phoneme"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(DISTINCT role) FROM allomorph_environment WHERE allomorph_guid=?1",
            [ALLO_SUFFIX],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT phoneme_guid FROM natural_class_effective_member WHERE natural_class_guid=?1 AND phoneme_guid IS NOT NULL ORDER BY phoneme_guid LIMIT 1",
            [NC_VOICED],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        PHONEME_K
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM natural_class_member WHERE natural_class_guid=?1",
            [NC_C],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM natural_class_effective_member WHERE natural_class_guid=?1",
            [NC_VOICED],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM feature_assignment AS a JOIN feature_structure AS fs USING (fs_id) WHERE fs.owner_kind='phoneme' AND fs.owner_guid=?1",
            [PHONEME_TS],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0,
        "phonemes without authored feature assignments remain unassigned"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM pattern_node WHERE root_id IN (SELECT root_id FROM pattern_root WHERE owner_guid=?1) AND kind='boundary' AND boundary_guid=?2",
            rusqlite::params![RULE_REWRITE, BOUNDARY_PLUS],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT status FROM artifact_section WHERE section='patterns'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "complete"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM environment_usage WHERE compiled=1 AND result<>'represented'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0
    );
    assert!(
        db.query_row(
            "SELECT COUNT(*) FROM pattern_root WHERE owner_guid=?1 AND role='rewrite_lhs'",
            [RULE_REWRITE],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
            > 0
    );
}

#[test]
fn preserves_invalid_dangling_and_owner_unreached_environment_facts() {
    const PHONEME_K: &str = "00000000-0000-0000-0000-000000000101";
    const PHONEME_M: &str = "00000000-0000-0000-0000-000000000103";

    let mut authored = snapshot();
    authored.phonology.natural_classes = vec![NaturalClass::Segments {
        guid: NC_C.into(),
        name: "C".into(),
        display_name: None,
        phonemes: vec![PHONEME_K.into(), PHONEME_M.into()],
    }];
    authored.phonology.environments = vec![
        Environment {
            guid: ENV_VALID.into(),
            name: "Valid but owner rejected first".into(),
            representation: "/#_".into(),
        },
        Environment {
            guid: ENV_REPEAT.into(),
            name: "Position".into(),
            representation: "/([C][C])_".into(),
        },
        Environment {
            guid: ENV_INVALID.into(),
            name: "Unknown class on right".into(),
            representation: "/[C]_[Missing]".into(),
        },
    ];
    authored.lexicon.entries[2].allomorphs[0].environments =
        vec![ENV_INVALID.into(), ENV_DANGLING.into()];
    authored.lexicon.entries[2].allomorphs[0].positions = vec![ENV_REPEAT.into()];
    authored.lexicon.entries.push(entry(
        ENTRY_OWNER_REJECTED,
        SENSE_OWNER_REJECTED,
        {
            let mut allo = allomorph(ALLO_OWNER_REJECTED, MorphType::Suffix, "[pattern]");
            allo.environments = vec![ENV_VALID.into()];
            allo
        },
        Msa::Inflectional {
            guid: MSA_OWNER_REJECTED.into(),
            part_of_speech: Some(POS.into()),
            slots: vec![SLOT.into()],
            features: None,
            exception_features: Vec::new(),
        },
        "rejected owner",
    ));

    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("refused-environment-facts.sqlite");
    let result = build(&output, authored.to_json().as_bytes(), context()).unwrap();
    assert_eq!(result.compile_status, "refused");
    let db = Connection::open(output).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT parse_status FROM environment WHERE guid=?1",
            [ENV_INVALID],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "invalid"
    );
    assert_eq!(
        db.query_row(
            "SELECT parse_error_code FROM environment WHERE guid=?1",
            [ENV_INVALID],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "invalid_environment"
    );
    assert_eq!(
        db.query_row(
            "SELECT result || ':' || natural_class_guid FROM environment_natural_class WHERE environment_guid=?1 AND token_text='[C]'",
            [ENV_INVALID],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        format!("resolved:{NC_C}"),
        "the left class token stays resolved when the right side invalidates the expression"
    );
    assert_eq!(
        db.query_row(
            "SELECT result FROM environment_natural_class WHERE environment_guid=?1 AND token_text='[Missing]'",
            [ENV_INVALID],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "unresolved"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM pattern_root WHERE owner_kind='environment' AND owner_guid=?1",
            [ENV_INVALID],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0,
        "the invalid whole expression yields no partial pattern roots"
    );
    assert_eq!(
        db.query_row(
            "SELECT result FROM environment_usage WHERE allomorph_guid=?1 AND environment_guid=?2",
            rusqlite::params![ALLO_SUFFIX, ENV_DANGLING],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "unresolved"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(DISTINCT role) FROM allomorph_environment WHERE allomorph_guid=?1",
            [ALLO_SUFFIX],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT result FROM environment_usage WHERE allomorph_guid=?1 AND environment_guid=?2",
            rusqlite::params![ALLO_OWNER_REJECTED, ENV_VALID],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "owner_not_loaded"
    );
    assert_eq!(
        db.query_row(
            "SELECT parse_status FROM environment WHERE guid=?1",
            [ENV_VALID],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "not_attempted",
        "an owner rejected before condition resolution must not claim a parse result"
    );
    assert_eq!(
        db.query_row(
            "SELECT status FROM artifact_section WHERE section='patterns'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "partial"
    );
}

#[test]
fn exports_one_validated_frozen_stats_run_with_support_and_source_bridges() {
    let temp = tempfile::tempdir().unwrap();
    let mut snapshot = snapshot();
    snapshot
        .lexicon
        .entries
        .iter_mut()
        .find(|entry| entry.guid == ENTRY_ROOT)
        .unwrap()
        .allomorphs
        .insert(0, allomorph(ALLO_ROOT_ALT, MorphType::Root, "suma"));
    snapshot.lexicon.entries.push(LexEntry {
        guid: ENTRY_CIRCUMFIX.into(),
        citation_form: vec![ws("qaa", "ka-...-ta")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![
            allomorph(ALLO_CIRCUMFIX_PREFIX, MorphType::Prefix, "ka"),
            allomorph(ALLO_CIRCUMFIX_SUFFIX, MorphType::Suffix, "ta"),
        ],
        msas: vec![Msa::Inflectional {
            guid: MSA_CIRCUMFIX.into(),
            part_of_speech: Some(POS.into()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    });
    snapshot
        .morphology
        .compound_rules
        .push(CompoundRule::Endocentric {
            guid: COMPOUND_RULE.into(),
            name: "Fixture compound".into(),
            disabled: false,
            head_last: false,
            left: Default::default(),
            right: Default::default(),
            overriding: CompoundOutcome::default(),
        });
    let source = snapshot.to_json().into_bytes();
    let output = temp.path().join("facts.sqlite");
    let cache_path = temp.path().join("stats.sqlite");
    let manifest_path = temp.path().join("stats.json");
    let (grammar, _) = pg_grammar::compile_project(&snapshot).unwrap();
    let step_cap = "200000".parse::<StepCap>().unwrap();
    let options = serde_json::json!({
        "engine": "hc",
        "step_cap": step_cap,
        "work_cap": 2_000_000,
        "search_budget_semantics": 2,
        "word_timeout_ms": null,
        "guess": false,
        "always_enforce_final_templates": false,
    });
    let options_json = serde_json::to_string(&options).unwrap();
    let options_hash = pg_assess::source_sha256(options_json.as_bytes())
        .strip_prefix("sha256:")
        .unwrap()
        .to_string();
    let cache = StatsCache::open(&cache_path, &snapshot.grammar_hash()).unwrap();
    let mut cache = cache.cache;
    let entry_index = grammar
        .entries
        .iter()
        .position(|entry| entry.authored_id == ENTRY_ROOT)
        .unwrap();
    let entry_id = pg_grammar::model::LexEntryId(entry_index as u32);
    let object = pg_grammar::stats_identity::lex_entry_identity(&grammar, entry_id);
    let entry_def = &grammar.entries[entry_index];
    let morpheme = pg_grammar::stats_identity::morpheme_identity(&grammar, entry_def.morpheme);
    let stratum = pg_grammar::stats_identity::stratum_identity(
        &grammar,
        morpheme_stratum(&grammar, entry_def.morpheme),
    );
    let allomorph_id = entry_def.allomorphs[0].id;
    let allomorph = pg_grammar::stats_identity::allomorph_identity(&grammar, allomorph_id);
    let circumfix_rule_index = grammar
        .mrules
        .iter()
        .position(|rule| match rule {
            pg_grammar::model::MorphRuleDef::AffixProcess(def) => {
                grammar.morphemes[def.morpheme.0 as usize]
                    .source_msa_guid
                    .as_deref()
                    == Some(MSA_CIRCUMFIX)
            }
            _ => false,
        })
        .unwrap();
    let circumfix_rule_id = pg_grammar::model::MRuleId(circumfix_rule_index as u32);
    let circumfix_rule =
        pg_grammar::stats_identity::morph_rule_identity(&grammar, circumfix_rule_id);
    let pg_grammar::model::MorphRuleDef::AffixProcess(circumfix_def) =
        &grammar.mrules[circumfix_rule_index]
    else {
        panic!("the circumfix MSA must compile to an affix-process rule");
    };
    assert_eq!(circumfix_def.allomorphs.len(), 1);
    let circumfix_stratum = pg_grammar::stats_identity::stratum_identity(
        &grammar,
        morpheme_stratum(&grammar, circumfix_def.morpheme),
    );
    let circumfix_allomorph =
        pg_grammar::stats_identity::allomorph_identity(&grammar, circumfix_def.allomorphs[0].id);
    let compound_rule_index = grammar
        .mrules
        .iter()
        .position(|rule| match rule {
            pg_grammar::model::MorphRuleDef::Compounding(def) => {
                def.source_guid.as_deref() == Some(COMPOUND_RULE)
            }
            _ => false,
        })
        .unwrap();
    let compound_rule = pg_grammar::stats_identity::morph_rule_identity(
        &grammar,
        pg_grammar::model::MRuleId(compound_rule_index as u32),
    );
    let guesser = pg_grammar::stats_identity::guesser_identity(&grammar);
    let run_id = cache
        .flush(
            &RunMetadata {
                build_info: producer().build_identity.into(),
                fwdata_path: "fixture.fwdata".into(),
                grammar_hash: snapshot.grammar_hash(),
                engine: "hc".into(),
                options_hash: options_hash.clone(),
                options_json: options_json.clone(),
                created_utc: "unix:1".into(),
                step_cap: Some(step_cap),
            },
            &[
                WordRecord {
                    form: "kuma".into(),
                    elapsed_ns: 23,
                    attempts: 4,
                    passes: 1,
                    capped: false,
                    timed_out: false,
                    invalid_shape: false,
                    facts: vec![
                        FactRecord {
                            object_key: object.key,
                            object_kind: ObjectKind::LexEntry,
                            object_label: object.label,
                            identity_quality: IdentityQuality::Authored,
                            stratum: Some(StructuralLocator::new(stratum.key, stratum.label)),
                            allomorph: Some(StructuralLocator::new(allomorph.key, allomorph.label)),
                            morpheme: Some(StructuralLocator::new(morpheme.key, morpheme.label)),
                            direction: pg_stats::Direction::Analysis,
                            attempts: 1,
                            work: 2,
                            outputs: 0,
                            not_applied: 0,
                            no_root: 0,
                            surface_mismatch: 0,
                            uses: 0,
                            self_time_ns: 5,
                        },
                        FactRecord {
                            object_key: circumfix_rule.key.clone(),
                            object_kind: ObjectKind::MorphRule,
                            object_label: circumfix_rule.label.clone(),
                            identity_quality: IdentityQuality::Authored,
                            stratum: Some(StructuralLocator::new(
                                circumfix_stratum.key,
                                circumfix_stratum.label,
                            )),
                            allomorph: Some(StructuralLocator::new(
                                circumfix_allomorph.key,
                                circumfix_allomorph.label,
                            )),
                            morpheme: None,
                            direction: pg_stats::Direction::Analysis,
                            attempts: 2,
                            work: 9,
                            outputs: 1,
                            not_applied: 0,
                            no_root: 0,
                            surface_mismatch: 0,
                            uses: 1,
                            self_time_ns: 7,
                        },
                        FactRecord {
                            object_key: compound_rule.key.clone(),
                            object_kind: ObjectKind::MorphRule,
                            object_label: compound_rule.label.clone(),
                            identity_quality: IdentityQuality::Authored,
                            stratum: None,
                            allomorph: None,
                            morpheme: None,
                            direction: pg_stats::Direction::Analysis,
                            attempts: 1,
                            work: 3,
                            outputs: 1,
                            not_applied: 0,
                            no_root: 0,
                            surface_mismatch: 0,
                            uses: 1,
                            self_time_ns: 2,
                        },
                        FactRecord {
                            object_key: guesser.key,
                            object_kind: ObjectKind::Guesser,
                            object_label: guesser.label,
                            identity_quality: IdentityQuality::Synthetic,
                            stratum: None,
                            allomorph: None,
                            morpheme: None,
                            direction: pg_stats::Direction::Analysis,
                            attempts: 1,
                            work: 1,
                            outputs: 1,
                            not_applied: 0,
                            no_root: 0,
                            surface_mismatch: 0,
                            uses: 1,
                            self_time_ns: 1,
                        },
                    ],
                },
                WordRecord {
                    form: "capped-form".into(),
                    elapsed_ns: 30,
                    attempts: 5,
                    passes: 2,
                    capped: true,
                    timed_out: false,
                    invalid_shape: false,
                    facts: Vec::new(),
                },
                WordRecord {
                    form: "timed-out-form".into(),
                    elapsed_ns: 40,
                    attempts: 6,
                    passes: 3,
                    capped: false,
                    timed_out: true,
                    invalid_shape: false,
                    facts: Vec::new(),
                },
                WordRecord {
                    form: "invalid-shape-form".into(),
                    elapsed_ns: 10,
                    attempts: 0,
                    passes: 0,
                    capped: false,
                    timed_out: false,
                    invalid_shape: true,
                    facts: Vec::new(),
                },
            ],
        )
        .unwrap();
    cache.checkpoint_and_close().unwrap();

    let compile_options = pg_grammar::compile::CompileOptions::default();
    let parameters = &snapshot.morphology.parser_parameters;
    let resolved = compile_options.substrate.resolve(
        parameters.active_parser,
        parameters.accept_unspecified_graphemes,
    );
    let compile_options_json = compile_options.canonical_projection_json(resolved);
    let fingerprint = pg_assess::model_fingerprint(
        pg_assess::SourceKind::Snapshot,
        std::str::from_utf8(&source).unwrap(),
        producer().compiler_version,
    )
    .unwrap();
    let cache_bytes = std::fs::read(&cache_path).unwrap();
    let words = vec![
        "kuma".to_string(),
        "capped-form".to_string(),
        "timed-out-form".to_string(),
        "invalid-shape-form".to_string(),
        "missing-form".to_string(),
    ];
    let word_bytes = serde_json::to_vec(&words).unwrap();
    let manifest = serde_json::json!({
        "format": "pangloss-batch-stats-manifest",
        "version": 1,
        "source": {
            "kind": "snapshot",
            "source_sha256": pg_assess::source_sha256(&source),
            "grammar_hash": snapshot.grammar_hash(),
            "model_fingerprint": fingerprint,
            "compile_options_json": compile_options_json,
        },
        "compiler": {
            "version": producer().compiler_version,
            "build_identity": producer().build_identity,
        },
        "cache": {
            "sha256": pg_assess::source_sha256(&cache_bytes),
            "bytes": cache_bytes.len(),
            "schema_version": pg_stats::SCHEMA_VERSION,
            "counter_semantics_version": pg_stats::COUNTER_SEMANTICS_VERSION,
        },
        "run": {
            "id": run_id,
            "engine": "hc",
            "grammar_hash": snapshot.grammar_hash(),
            "options_hash": options_hash,
            "options_json": options_json,
        },
        "batch": {
            "engine": "hc",
            "threads": 1,
            "step_cap": "200000",
            "work_cap": 2_000_000,
            "search_budget_semantics": 2,
            "word_timeout_ms": null,
            "guess": false,
            "always_enforce_final_templates": false,
            "start": 0,
            "analyses_requested": false,
        },
        "input": {
            "word_count": words.len(),
            "word_list_sha256": pg_assess::source_sha256(&word_bytes),
            "words": words,
        },
        "completion": {
            "requested": words.len(),
            "complete": 1,
            "incomplete": 2,
            "invalid_shape": 1,
            "missing": 1,
            "words": [{
                "index": 0,
                "form": "kuma",
                "status": "complete",
                "capped": false,
                "timed_out": false,
                "invalid_shape": false,
            }, {
                "index": 1,
                "form": "capped-form",
                "status": "incomplete",
                "capped": true,
                "timed_out": false,
                "invalid_shape": false,
            }, {
                "index": 2,
                "form": "timed-out-form",
                "status": "incomplete",
                "capped": false,
                "timed_out": true,
                "invalid_shape": false,
            }, {
                "index": 3,
                "form": "invalid-shape-form",
                "status": "invalid_shape",
                "capped": false,
                "timed_out": false,
                "invalid_shape": true,
            }, {
                "index": 4,
                "form": "missing-form",
                "status": "missing",
                "capped": false,
                "timed_out": false,
                "invalid_shape": false,
            }],
        },
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let result = pg_facts::write_facts_with_stats(
        &source,
        context(),
        &output,
        producer(),
        pg_facts::StatsInput::new(&cache_path, &manifest_path),
    )
    .unwrap();
    assert_eq!(result.schema_version, 7);
    assert_eq!(
        result
            .sections
            .iter()
            .find(|s| s.section == "stats")
            .unwrap()
            .status,
        "complete"
    );
    let db = Connection::open(&output).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT f.uses FROM stats_fact f JOIN stats_object o USING (object_id) WHERE o.kind='lex_entry'",
            [],
            |row| row.get::<_, i64>(0),
        )
            .unwrap(),
        0,
        "a measured zero remains a numeric zero"
    );
    assert_eq!(
        db.query_row(
            "SELECT support FROM stats_counter_support WHERE object_kind='lex_entry' AND counter='uses' AND direction='both'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "measured"
    );
    assert_eq!(
        db.query_row(
            "SELECT support FROM stats_counter_support WHERE object_kind='phon_rule' AND counter='uses' AND direction='both'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "not_wired",
        "PhonRule uses must not turn its stored default zero into a measured fact"
    );
    assert_eq!(
        db.query_row(
            "SELECT s.source_guid FROM stats_object_source s JOIN stats_object o USING (object_id) WHERE o.kind='lex_entry' AND s.source_kind='entry' AND s.role='owner_entry'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        ENTRY_ROOT
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM stats_fact", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        4,
        "each cache fact must be projected exactly once"
    );
    let word_statuses: Vec<(String, String)> = db
        .prepare("SELECT form, status FROM stats_word ORDER BY word_id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        word_statuses,
        vec![
            ("kuma".into(), "complete".into()),
            ("capped-form".into(), "incomplete".into()),
            ("timed-out-form".into(), "incomplete".into()),
            ("invalid-shape-form".into(), "invalid_shape".into()),
            ("missing-form".into(), "not_attempted".into()),
        ],
        "cap, timeout, invalid-shape and missing states remain distinct"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM stats_fact f JOIN stats_word w USING (word_id) WHERE w.status <> 'complete'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0,
        "no counter row should make an incomplete, invalid, or missing word look measured"
    );
    let (missing_status, missing_elapsed, missing_attempts): (String, Option<i64>, Option<i64>) =
        db.query_row(
            "SELECT status, elapsed_ns, attempts FROM stats_word WHERE form='missing-form'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(missing_status, "not_attempted");
    assert_eq!((missing_elapsed, missing_attempts), (None, None));
    let msa_roles: Vec<String> = db
        .prepare("SELECT role FROM stats_object_source WHERE source_kind='msa' AND source_guid=?1 ORDER BY role")
        .unwrap()
        .query_map([MSA_CIRCUMFIX], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(msa_roles, vec!["morpheme_msa"]);
    let entry_roles: Vec<String> = db
        .prepare("SELECT role FROM stats_object_source WHERE object_id=(SELECT object_id FROM stats_object WHERE key=?1 AND kind='morph_rule') AND source_kind='entry' AND source_guid=?2 ORDER BY role")
        .unwrap()
        .query_map([circumfix_rule.key.as_str(), ENTRY_CIRCUMFIX], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        entry_roles,
        vec!["msa_owner_entry", "source_form_owner_entry"]
    );
    assert_eq!(
        db.query_row(
            "SELECT s.source_allomorph_guid FROM stats_fact f JOIN stats_object o USING (object_id) JOIN stats_allomorph_source s USING (allomorph_id) WHERE o.kind='lex_entry' AND o.key=?1 AND s.source_ordinal=0",
            [ENTRY_ROOT],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        ALLO_ROOT_ALT,
        "a reordered authored allomorph retains its exact source GUID at the compiled order"
    );
    let compound_source: (String, String, String) = db
        .query_row(
            "SELECT s.source_kind, s.source_guid, s.role FROM stats_object_source s JOIN stats_object o USING (object_id) WHERE o.key=?1 AND o.kind='morph_rule'",
            [compound_rule.key.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        compound_source,
        (
            "compoundRule".into(),
            COMPOUND_RULE.into(),
            "compound_rule".into(),
        ),
        "an authored compound rule keeps its exact typed source bridge"
    );
    assert_eq!(
        db.query_row(
            "SELECT identity_quality FROM stats_object WHERE key='guesser'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "synthetic"
    );
    let (guess_stratum, guess_allomorph, guess_morpheme): (i64, i64, i64) = db
        .query_row(
            "SELECT f.stratum_id, f.allomorph_id, o.morpheme_id FROM stats_fact f JOIN stats_object o USING (object_id) WHERE o.key='guesser'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (guess_stratum, guess_allomorph, guess_morpheme),
        (0, 0, 0),
        "synthetic guesser facts use explicit not-applicable dimension sentinels"
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM stats_allomorph WHERE allomorph_id=0 AND key IS NULL AND identity_quality IS NULL",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    let circumfix_sources: Vec<(i64, String, String)> = db
        .prepare("SELECT s.source_ordinal, s.source_allomorph_guid, s.role FROM stats_fact f JOIN stats_object o USING (object_id) JOIN stats_allomorph_source s USING (allomorph_id) WHERE o.kind='morph_rule' AND o.key=?1 ORDER BY s.source_ordinal")
        .unwrap()
        .query_map([circumfix_rule.key.as_str()], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        circumfix_sources,
        vec![
            (0, ALLO_CIRCUMFIX_PREFIX.into(), "prefix".into()),
            (1, ALLO_CIRCUMFIX_SUFFIX.into(), "suffix".into()),
        ],
        "a compiled circumfix product retains both typed source halves"
    );
    assert_eq!(
        db.query_row(
            "SELECT SUM(f.work) FROM stats_fact f JOIN stats_object o USING (object_id) WHERE o.key=?1 AND EXISTS (SELECT 1 FROM stats_allomorph_source s WHERE s.allomorph_id=f.allomorph_id AND s.source_allomorph_guid IN (?2, ?3))",
            [circumfix_rule.key.as_str(), ALLO_CIRCUMFIX_PREFIX, ALLO_CIRCUMFIX_SUFFIX],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        9,
        "joining the two circumfix halves must not double its work"
    );
    let manifest_digest: String = db
        .query_row("SELECT run_manifest_sha256 FROM artifact_meta", [], |row| {
            row.get(0)
        })
        .unwrap();
    let manifest_bytes = std::fs::read(&manifest_path).unwrap();
    assert_eq!(manifest_digest, pg_assess::source_sha256(&manifest_bytes));

    let repeated_output = temp.path().join("facts-repeat.sqlite");
    pg_facts::write_facts_with_stats(
        &source,
        context(),
        &repeated_output,
        producer(),
        pg_facts::StatsInput::new(&cache_path, &manifest_path),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(&output).unwrap(),
        std::fs::read(&repeated_output).unwrap(),
        "identical frozen input and producer identity must produce identical database bytes"
    );
}

fn morpheme_stratum(
    grammar: &pg_grammar::model::Grammar,
    id: pg_grammar::model::MorphemeId,
) -> pg_grammar::model::StratumId {
    grammar.morphemes[id.0 as usize].stratum
}
