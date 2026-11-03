//! `grammar.limit.*` errors at each bitset cap and one past it (issue #150), built from code-constructed snapshots.

use pg_snapshot::feature::{ClosedFeature, FeatureSystem, FeatureValueSymbol};
use pg_snapshot::morphology::{ExceptionFeature, InflectionClass, Morphology, PartOfSpeech};
use pg_snapshot::{ImportWarningCode, Snapshot};

use super::compile_project;
use crate::GrammarError;

use super::tests::fixture_snapshot;

fn pos(guid: String, children: Vec<PartOfSpeech>, classes: Vec<InflectionClass>) -> PartOfSpeech {
    PartOfSpeech {
        guid: guid.clone(),
        name: guid,
        abbreviation: String::new(),
        children,
        inflection_classes: classes,
        default_inflection_class: None,
        inflectable_features: Vec::new(),
        stem_names: Vec::new(),
        affix_slots: Vec::new(),
        affix_templates: Vec::new(),
    }
}

fn class(guid: String, children: Vec<InflectionClass>) -> InflectionClass {
    InflectionClass {
        name: guid.clone(),
        abbreviation: guid.clone(),
        guid,
        children,
    }
}

/// `n` parts of speech in total: top-level categories alternate with one subcategory each, so subcategories count.
fn parts_of_speech(n: usize) -> Vec<PartOfSpeech> {
    let mut out = Vec::new();
    let mut made = 0;
    while made < n {
        let children = if made + 1 < n {
            vec![pos(format!("sub{}", made + 1), Vec::new(), Vec::new())]
        } else {
            Vec::new()
        };
        made += 1 + children.len();
        out.push(pos(format!("top{made}"), children, Vec::new()));
    }
    out
}

fn closed(n: usize) -> ClosedFeature {
    ClosedFeature {
        guid: "feat".into(),
        name: "case".into(),
        abbreviation: "case".into(),
        values: (0..n)
            .map(|i| FeatureValueSymbol {
                guid: format!("v{i}"),
                name: format!("v{i}"),
                abbreviation: format!("v{i}"),
            })
            .collect(),
    }
}

fn with_morphology(f: impl FnOnce(&mut Morphology)) -> Snapshot {
    let mut snapshot = fixture_snapshot();
    snapshot.morphology.parts_of_speech.clear();
    snapshot.lexicon.entries.clear();
    f(&mut snapshot.morphology);
    snapshot
}

fn limit_code(snapshot: &Snapshot) -> Option<(ImportWarningCode, String)> {
    match compile_project(snapshot) {
        Err(GrammarError::Limit { code, message }) => Some((code, message)),
        Err(other) => panic!("expected a limit error or success, got {other}"),
        Ok(_) => None,
    }
}

#[test]
fn parts_of_speech_cap_63_compiles_and_64_names_the_count() {
    let at_cap = with_morphology(|m| m.parts_of_speech = parts_of_speech(63));
    assert_eq!(limit_code(&at_cap), None);
    let over = with_morphology(|m| m.parts_of_speech = parts_of_speech(64));
    let (code, message) = limit_code(&over).expect("64 parts of speech must be refused");
    assert_eq!(code, ImportWarningCode::LimitPartsOfSpeech);
    assert!(
        message.contains("64 parts of speech including subcategories")
            && message.contains("at most 63"),
        "{message}"
    );
}

#[test]
fn feature_value_cap_63_compiles_and_64_names_the_feature() {
    let mut at_cap = fixture_snapshot();
    at_cap.feature_systems.morphosyntactic.closed_features = vec![closed(63)];
    assert_eq!(limit_code(&at_cap), None);
    let mut over = fixture_snapshot();
    over.feature_systems.morphosyntactic.closed_features = vec![closed(64)];
    let (code, message) = limit_code(&over).expect("64 values must be refused");
    assert_eq!(code, ImportWarningCode::LimitFeatureValues);
    assert!(
        message.contains("'case'")
            && message.contains("has 64 values")
            && message.contains("at most 63"),
        "{message}"
    );
}

#[test]
fn phonological_feature_value_cap_63_compiles_and_64_is_refused() {
    let phon = |n| FeatureSystem {
        closed_features: vec![closed(n)],
        complex_features: Vec::new(),
    };
    let mut at_cap = fixture_snapshot();
    at_cap.feature_systems.phonological = phon(63);
    assert_eq!(limit_code(&at_cap), None);
    let mut over = fixture_snapshot();
    over.feature_systems.phonological = phon(64);
    let (code, message) = limit_code(&over).expect("64 values must be refused");
    assert_eq!(code, ImportWarningCode::LimitFeatureValues);
    assert!(message.contains("phonological feature 'case'"), "{message}");
}

/// `n` MPR features: nested inflection classes under one category, the rest exception features.
fn mpr_morphology(n: usize) -> impl FnOnce(&mut Morphology) {
    move |m| {
        let classes = 40.min(n);
        let nested = class(
            "c0".into(),
            (1..classes)
                .map(|i| class(format!("c{i}"), Vec::new()))
                .collect(),
        );
        m.parts_of_speech = vec![pos("p".into(), Vec::new(), vec![nested])];
        m.exception_features = (0..n - classes)
            .map(|i| ExceptionFeature {
                guid: format!("e{i}"),
                name: format!("e{i}"),
                abbreviation: format!("e{i}"),
            })
            .collect();
    }
}

#[test]
fn mpr_cap_64_compiles_and_65_names_the_breakdown() {
    assert_eq!(limit_code(&with_morphology(mpr_morphology(64))), None);
    let (code, message) =
        limit_code(&with_morphology(mpr_morphology(65))).expect("65 MPR features must be refused");
    assert_eq!(code, ImportWarningCode::LimitMprFeatures);
    assert!(
        message.contains("65 inflection classes, exception features")
            && message.contains(
                "40 inflection classes including subclasses, 25 exception features, 0 entry"
            )
            && message.contains("at most 64"),
        "{message}"
    );
}
