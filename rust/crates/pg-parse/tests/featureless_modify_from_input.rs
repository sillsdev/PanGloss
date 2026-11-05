//! `ModifyFromInput` through a segment class, undone in analysis (issue #121). C# adds the context's `AntiFeatureStruct()` to the node; that leaves `StrRep` as the node's own spelling united with the context's, and only a segment without authored features has one. So in a featureless grammar a root is recovered only if its spelling is already in the node's set. With a feature system the segments carry no `StrRep` and the same grammar recovers every root. The grammar is the staged fixture `featureless-modify-from-input`; every verdict below is from the C# oracle.

use pg_grammar::load;
use pg_grammar_model::model::Grammar;
use pg_parse::Morpher;

fn featureless() -> String {
    pg_conformance_fixtures::require_fixture("edge-cases", "featureless-modify-from-input")
        .load_grammar_xml()
}

/// The staged grammar with one symbolic feature that gives every segment its own symbol.
fn featured() -> String {
    let segs = ["k", "t", "p", "s", "a", "i"];
    let symbols: String = segs
        .iter()
        .map(|c| format!(r#"<Symbol id="f{c}">{c}</Symbol>"#))
        .collect();
    let mut xml = featureless().replacen(
        "<CharacterDefinitionTable",
        &format!(
            r#"<PhonologicalFeatureSystem><SymbolicFeature id="featPh"><Name>ph</Name><Symbols>{symbols}</Symbols></SymbolicFeature></PhonologicalFeatureSystem><CharacterDefinitionTable"#
        ),
        1,
    );
    for c in segs {
        let old = format!(
            r#"<Representation>{c}</Representation></Representations></SegmentDefinition>"#
        );
        let new = format!(
            r#"<Representation>{c}</Representation></Representations><FeatureValue feature="featPh" symbolValues="f{c}" /></SegmentDefinition>"#
        );
        assert!(xml.contains(&old), "segment {c}");
        xml = xml.replacen(&old, &new, 1);
    }
    xml
}

fn parses(g: &Grammar, word: &str) -> usize {
    let outcome = Morpher::new(g, usize::MAX).parse_word(word);
    assert!(!outcome.capped && !outcome.invalid_shape, "{word}");
    outcome.analyses.len()
}

#[test]
fn featureless_undo_recovers_only_a_root_whose_spelling_is_in_the_class() {
    let g = load(&featureless()).expect("fixture grammar loads");
    assert!(
        g.phon_features.is_empty(),
        "the grammar must be featureless"
    );
    assert_eq!(
        parses(&g, "kitas"),
        1,
        "kita, not kata: the a is not recovered"
    );
    assert_eq!(parses(&g, "sikas"), 1, "sika already has the vowel i");
    assert_eq!(parses(&g, "kita"), 1, "the bare root");
    assert_eq!(parses(&g, "kata"), 1, "the bare root");
    for word in ["tisis", "pisas", "kakas", "tata"] {
        assert_eq!(parses(&g, word), 0, "{word}: hc.dll finds no analysis");
    }
}

#[test]
fn a_feature_system_recovers_every_root() {
    let g = load(&featured()).expect("featured grammar loads");
    assert!(
        !g.phon_features.is_empty(),
        "the grammar must have features"
    );
    assert_eq!(parses(&g, "kitas"), 2, "kita and kata");
    for word in ["sikas", "tisis", "pisas"] {
        assert_eq!(parses(&g, word), 1, "{word}");
    }
    for word in ["kakas", "tata"] {
        assert_eq!(parses(&g, word), 0, "{word}");
    }
}
