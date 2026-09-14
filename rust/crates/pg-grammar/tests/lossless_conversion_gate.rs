//! Owner-effect tests for `pg_grammar::compile`'s fatal-vs-recall-gap classification: a construct whose loss changes what the compiled grammar MEANS must refuse, while one allomorph's own unrepresentability is a recall gap, never a whole-project refusal.

use pg_snapshot::lexicon::{Allomorph, LexEntry, Lexicon, Msa, Sense};
use pg_snapshot::morphology::{AdhocProhibition, Adjacency, Morphology, PartOfSpeech};
use pg_snapshot::phonology::{BoundaryMarker, Phoneme, Phonology};
use pg_snapshot::project::Project;
use pg_snapshot::{FeatureSystems, MorphType, Snapshot, WsForm};

use pg_grammar::compile::{CompileOptions, SemanticLossPolicy};
use pg_grammar::compile_project_with;

fn ws(ws: &str, form: &str) -> WsForm {
    WsForm { ws: ws.to_string(), form: form.to_string() }
}

fn phoneme(guid: &str, rep: &str) -> Phoneme {
    Phoneme { guid: guid.to_string(), name: rep.to_string(), representations: vec![ws("sen", rep)], features: None, basic_ipa_symbol: None }
}

fn boundary(guid: &str, rep: &str) -> BoundaryMarker {
    BoundaryMarker { guid: guid.to_string(), name: rep.to_string(), representations: vec![ws("sen", rep)] }
}

fn simple_allomorph(guid: &str, morph_type: MorphType, form: &str) -> Allomorph {
    Allomorph {
        guid: guid.to_string(),
        morph_type,
        is_abstract: false,
        forms: vec![ws("sen", form)],
        environments: Vec::new(),
        positions: Vec::new(),
        stem_name: None,
        inflection_classes: Vec::new(),
        ms_env_features: None,
        ms_env_part_of_speech: None,
        process: None,
    }
}

/// Two stems ("kuma", "sipi") and one suffix ("-ta"), all built from segments this fixture's phonology declares -- a minimal two-morpheme project real enough to attach ad-hoc co-occurrence rules to.
fn two_stem_fixture() -> Snapshot {
    let noun_pos = PartOfSpeech {
        guid: "pos-noun".to_string(),
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
    let stem_a = LexEntry {
        guid: "entry-a".to_string(),
        citation_form: vec![ws("sen", "kuma")],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![simple_allomorph("allo-a", MorphType::Stem, "kuma")],
        msas: vec![Msa::Stem {
            guid: "msa-a".to_string(),
            part_of_speech: Some(noun_pos.guid.clone()),
            inflection_class: None,
            features: None,
            exception_features: Vec::new(),
            from_parts_of_speech: Vec::new(),
            slots: Vec::new(),
        }],
        senses: vec![Sense { guid: "sense-a".to_string(), gloss: vec![ws("en", "dog")], definition: Vec::new(), msa: Some("msa-a".to_string()) }],
        entry_refs: Vec::new(),
    };
    let stem_b = LexEntry {
        guid: "entry-b".to_string(),
        citation_form: vec![ws("sen", "sipi")],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![simple_allomorph("allo-b", MorphType::Stem, "sipi")],
        msas: vec![Msa::Stem {
            guid: "msa-b".to_string(),
            part_of_speech: Some(noun_pos.guid.clone()),
            inflection_class: None,
            features: None,
            exception_features: Vec::new(),
            from_parts_of_speech: Vec::new(),
            slots: Vec::new(),
        }],
        senses: vec![Sense { guid: "sense-b".to_string(), gloss: vec![ws("en", "cat")], definition: Vec::new(), msa: Some("msa-b".to_string()) }],
        entry_refs: Vec::new(),
    };

    Snapshot::new(
        Project {
            name: "Test".to_string(),
            vernacular_writing_systems: vec!["sen".to_string()],
            analysis_writing_systems: vec!["en".to_string()],
            exemplar_characters: Vec::new(),
        },
        FeatureSystems::default(),
        Phonology {
            phonemes: vec![phoneme("ph-k", "k"), phoneme("ph-t", "t"), phoneme("ph-m", "m"), phoneme("ph-s", "s"), phoneme("ph-p", "p"), phoneme("ph-a", "a"), phoneme("ph-i", "i"), phoneme("ph-u", "u")],
            boundary_markers: vec![boundary("bd-plus", "+")],
            ..Phonology::default()
        },
        Morphology { parts_of_speech: vec![noun_pos], ..Morphology::default() },
        Lexicon { entries: vec![stem_a, stem_b] },
    )
}

fn allomorph_prohibition(guid: &str, primary: &str, others: Vec<String>) -> AdhocProhibition {
    AdhocProhibition::Allomorph { guid: guid.to_string(), disabled: false, primary: primary.to_string(), others, adjacency: Adjacency::Anywhere }
}

// --- family: ad-hoc co-occurrence dangling reference -----------------------------------------

/// A prohibition attached to a REAL, compiled allomorph whose partner target never resolves must refuse: silently dropping it would let a combination through that was authored to be prohibited -- a meaning change, not a recall gap.
#[test]
fn dangling_cooccurrence_target_on_an_active_allomorph_refuses() {
    let mut snapshot = two_stem_fixture();
    snapshot.morphology.adhoc_prohibitions.push(allomorph_prohibition(
        "coocc-dangling",
        "allo-a",
        vec!["allo-does-not-exist".to_string()],
    ));

    let err = compile_project_with(&snapshot, CompileOptions::default())
        .expect_err("a dropped prohibition on an active allomorph must refuse the compile");
    assert!(err.issues().iter().any(|i| i.code == "grammar.adhoc-prohibition.unresolved" && i.fatal));
}

/// The SAME dangling target, but the primary itself never resolves to anything the compiled grammar keeps: the rule was never going to attach to anything real, so it is ignored with recorded provenance, not refused. Paired control for the test above -- same owner function, same code, opposite primary.
#[test]
fn dangling_cooccurrence_whose_primary_never_resolves_is_ignored_not_refused() {
    let mut snapshot = two_stem_fixture();
    snapshot.morphology.adhoc_prohibitions.push(allomorph_prohibition(
        "coocc-primary-dangling",
        "allo-does-not-exist",
        vec!["allo-does-not-exist-either".to_string()],
    ));

    let out = compile_project_with(&snapshot, CompileOptions::default())
        .expect("an unresolved primary attaches to nothing real, so this must not refuse");
    // Non-fatal recorder issues live in `inventory.issues`, never the top-level `issues` (that field folds in only the fatal half -- see `compile_project_with`'s own doc).
    assert!(out.inventory.issues.iter().any(|i| i.code == "grammar.adhoc-prohibition.unresolved" && !i.fatal));
}

/// A disabled rule must not refuse merely because its record (and its dangling target) exists -- disabled means never selected, so the owner never even reaches the resolution check.
#[test]
fn disabled_cooccurrence_rule_with_a_dangling_target_does_not_refuse() {
    let mut snapshot = two_stem_fixture();
    snapshot.morphology.adhoc_prohibitions.push(AdhocProhibition::Allomorph {
        guid: "coocc-disabled".to_string(),
        disabled: true,
        primary: "allo-a".to_string(),
        others: vec!["allo-does-not-exist".to_string()],
        adjacency: Adjacency::Anywhere,
    });

    let out = compile_project_with(&snapshot, CompileOptions::default())
        .expect("a disabled rule's own dangling reference must never refuse the compile");
    assert!(!out.issues.iter().any(|i| i.code == "grammar.adhoc-prohibition.unresolved"));
}

/// A fully-representable co-occurrence rule between two active allomorphs must succeed and attach -- the non-refusal control proving the refusal above is exact, not a broad preflight check.
#[test]
fn a_representable_cooccurrence_rule_between_two_active_allomorphs_compiles() {
    let mut snapshot = two_stem_fixture();
    snapshot.morphology.adhoc_prohibitions.push(allomorph_prohibition("coocc-real", "allo-a", vec!["allo-b".to_string()]));

    let out = compile_project_with(&snapshot, CompileOptions::default())
        .expect("a fully-resolved co-occurrence rule must compile");
    assert!(out.issues.iter().all(|i| i.code != "grammar.adhoc-prohibition.unresolved"));
    let has_rule = out
        .grammar
        .entries
        .iter()
        .flat_map(|e| &e.allomorphs)
        .any(|a| !a.co_occurrence.is_empty());
    assert!(has_rule, "the resolved rule must actually attach to its primary allomorph");
}

// --- family: substrate-unresolved literal text (per-allomorph recall gap) --------------------

/// An allomorph whose literal text cannot be segmented is a recall gap for that allomorph alone: it must not refuse the rest of the project, which is exactly the granularity decision this test module documents (see `pg_grammar::compile::substrate`'s module doc).
#[test]
fn unsegmentable_allomorph_text_is_a_recall_gap_not_a_project_refusal() {
    let mut snapshot = two_stem_fixture();
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "kuqa")]; // "q" is not declared anywhere in this fixture's phonology

    let out = compile_project_with(&snapshot, CompileOptions::default())
        .expect("one unrepresentable allomorph must not refuse the whole project");
    assert_eq!(out.grammar.entries.len(), 1, "only the unsegmentable stem is dropped");
    assert!(out.inventory.issues.iter().any(|i| i.code == "grammar.allomorph.unsegmentable" && !i.fatal));
}

/// Paired control: with no unrepresentable text, both stems compile -- the drop above is exact to the one bad allomorph, not a side effect of some broader change.
#[test]
fn both_stems_compile_when_nothing_is_unrepresentable() {
    let snapshot = two_stem_fixture();
    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("fixture must compile");
    assert_eq!(out.grammar.entries.len(), 2);
}

/// `MeasureOnly` must still surface the fatal co-occurrence refusal as an issue (never silently downgraded to a warning-only shape) even though it does not stop the compile -- see `SemanticLossPolicy::MeasureOnly`'s own doc.
#[test]
fn measure_only_still_reports_the_fatal_cooccurrence_issue() {
    let mut snapshot = two_stem_fixture();
    snapshot.morphology.adhoc_prohibitions.push(allomorph_prohibition(
        "coocc-dangling",
        "allo-a",
        vec!["allo-does-not-exist".to_string()],
    ));

    let out = compile_project_with(
        &snapshot,
        CompileOptions { semantic_loss: SemanticLossPolicy::MeasureOnly, ..CompileOptions::default() },
    )
    .expect("MeasureOnly never refuses");
    assert!(out.issues.iter().any(|i| i.code == "grammar.adhoc-prohibition.unresolved" && i.fatal));
}
