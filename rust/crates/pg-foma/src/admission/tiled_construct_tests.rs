//! Tiled acceptor soundness on small grammars, one per construct: no word the parser analyses is rejected.

use super::tests::{
    assimilation_spec, copy, grammar_xml, load, prefix, rule, spec, suffix, truncate_last, ANY_ONE,
    CTX_ANY,
};
use super::tiled_tests::{has_analysis, no_guess, setup, sweep, Setup};
use super::*;
use pg_parse::Morpher;

/// Sweeps the words and their edits: every analysed one is accepted, `analysed` are among them, and some non-word is rejected.
fn assert_sound(g: &Grammar, words: &[&str], analysed: &[&str]) -> Tiled {
    let Setup { admission, tiled } = setup(g);
    assert!(tiled.active(), "{:?}", tiled.inactive_reason());
    let (rejected, found) = sweep(g, &tiled, &admission, words);
    for word in &found {
        assert!(tiled.accepts(word), "{word:?} has an analysis");
    }
    for word in analysed {
        assert!(found.contains(&(*word).to_owned()), "{word} {found:?}");
    }
    assert!(!rejected.is_empty(), "nothing was rejected");
    tiled
}

/// Words that have no analysis and are rejected by the acceptor.
fn assert_rejected(g: &Grammar, tiled: &Tiled, words: &[&str]) {
    let Setup { admission, .. } = setup(g);
    let morpher = Morpher::new(g, 2_000_000);
    for word in words {
        let admitted = admission.admit(word);
        assert!(!has_analysis(&morpher, word), "{word}");
        assert!(tiled.rejects(word, &admitted, &no_guess()), "{word}");
    }
}

fn with_applications(rule: String, id: usize, count: usize) -> String {
    let at = format!(r#"<MorphologicalRule id="mr{id}""#);
    rule.replace(&at, &format!(r#"{at} multipleApplication="{count}""#))
}

fn drop_first(id: usize) -> String {
    rule(id, &[CTX_ANY, ANY_ONE], &[copy(1)])
}

fn compound(id: usize) -> String {
    let part =
        |name: &str| format!(r#"<PhoneticSequence id="{name}0">{ANY_ONE}</PhoneticSequence>"#);
    format!(
        r#"<CompoundingRule id="mr{id}" nonHeadPartsOfSpeech="posV"><Name>c{id}</Name><CompoundingSubrules><CompoundingSubrule>
        <HeadMorphologicalInput>{}</HeadMorphologicalInput><NonHeadMorphologicalInput>{}</NonHeadMorphologicalInput>
        <MorphologicalOutput><CopyFromInput index="h0" /><CopyFromInput index="n0" /></MorphologicalOutput>
        </CompoundingSubrule></CompoundingSubrules></CompoundingRule>"#,
        part("h"),
        part("n")
    )
}

/// The grammar with a stratum after the phonological one, holding the rules and, when given, root entries.
fn with_late_roots(xml: String, rules: (&[usize], &[String]), roots: &[&str]) -> Grammar {
    let listed: Vec<String> = rules.0.iter().map(|i| format!("mr{i}")).collect();
    let entries: String = roots.iter().enumerate().map(|(i, shape)| {
        format!(r#"<LexicalEntry id="late{i}" partOfSpeech="posV"><MorphemeId>late{i}</MorphemeId><Allomorphs><Allomorph id="lateA{i}"><PhoneticShape>{shape}</PhoneticShape></Allomorph></Allomorphs><Gloss>late{i}</Gloss></LexicalEntry>"#)
    }).collect();
    let stratum = format!(
        r#"<Stratum characterDefinitionTable="t" morphologicalRuleOrder="unordered" morphologicalRules="{}"><Name>Late</Name><MorphologicalRuleDefinitions>{}</MorphologicalRuleDefinitions><LexicalEntries>{entries}</LexicalEntries></Stratum></Strata>"#,
        listed.join(" "),
        rules.1.concat()
    );
    let xml = xml.replace("</Strata>", &stratum);
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

#[test]
fn two_windowed_edit_rules_on_one_word_stay_sound() {
    let rules = vec![
        prefix(0, "taa"),
        drop_first(1),
        truncate_last(2),
        suffix(3, "nu"),
    ];
    let g = load(&spec("taqkmnu", rules, &["qkm"]));
    let words = [
        "qkm", "km", "qk", "k", "taakm", "qknu", "taak", "knu", "taaqknu",
    ];
    let tiled = assert_sound(&g, &words, &words);
    let tiles = &tiled.evidence().tiles;
    assert_eq!((tiles.windowed, tiles.not_tileable), (2, 0));
    assert_eq!(tiles.edit_apps, 2);
    assert!(tiles.edit_converged && !tiles.edit_widened);
    assert_rejected(&g, &tiled, &["qq", "mm", "nuqkm", "kmk"]);
}

#[test]
fn a_rule_applying_twice_cuts_twice_and_no_more() {
    let twice = with_applications(drop_first(1), 1, 2);
    let g = load(&spec(
        "taqkpmnu",
        vec![prefix(0, "taaaa"), twice],
        &["qkpm"],
    ));
    let words = ["qkpm", "kpm", "pm", "taaaapm", "taaaakpm"];
    let tiled = assert_sound(&g, &words, &words);
    let tiles = &tiled.evidence().tiles;
    assert_eq!(
        (tiles.windowed, tiles.edit_apps, tiles.chop_left),
        (1, 2, 2)
    );
    assert_rejected(&g, &tiled, &["m", "taaaam", "qkm"]);
}

#[test]
fn an_edit_beside_a_phonological_rule_stays_sound() {
    let mut s = assimilation_spec(
        "pmnatku",
        vec![suffix(0, "na"), drop_first(1)],
        &["tap", "tat"],
    );
    s.boundary = false;
    let g = load(&s);
    let words = [
        "tapna", "tapma", "apma", "apna", "atna", "tatna", "ap", "at",
    ];
    let tiled = assert_sound(&g, &words, &["tapma", "apma", "tatna", "ap"]);
    assert_eq!(tiled.evidence().tiles.windowed, 1);
    assert_rejected(&g, &tiled, &["kk", "tapmu", "mm"]);
}

#[test]
fn a_root_reachable_only_after_phonology_is_accepted_as_written() {
    let xml = grammar_xml(&assimilation_spec(
        "pmnatku",
        vec![suffix(0, "na")],
        &["tap"],
    ));
    let g = with_late_roots(xml, (&[9], &[suffix(9, "ku")]), &["tapna", "tut"]);
    let words = ["tapna", "tapma", "tapnaku", "tut", "tutku", "tapku"];
    let tiled = assert_sound(&g, &words, &["tapna", "tapma", "tapnaku", "tut", "tutku"]);
    assert_eq!(tiled.evidence().tiles.deep_roots, 2);
    assert_rejected(&g, &tiled, &["tapnn", "kuku"]);
}

#[test]
fn an_outer_compounding_rule_repeats_whole_words_and_stays_sound() {
    let xml = grammar_xml(&assimilation_spec(
        "pmnatku",
        vec![suffix(0, "na")],
        &["tap", "tat"],
    ));
    let g = with_late_roots(xml, (&[9], &[compound(9)]), &["ku", "at"]);
    let words = [
        "tap", "tapma", "tapku", "tapmaku", "tapat", "ku", "at", "kuku",
    ];
    let tiled = assert_sound(&g, &words, &["tap", "tapma", "tapku", "tapmaku"]);
    assert_eq!(tiled.evidence().tiles.repeating, 1);
    assert_rejected(&g, &tiled, &["tapt", "mm", "kk"]);
}

#[test]
fn inner_compounding_joins_roots_and_stays_sound() {
    let g = load(&spec(
        "qtkm",
        vec![compound(0), suffix(1, "k")],
        &["qt", "tq", "m"],
    ));
    let words = ["qt", "qttq", "tqm", "mqt", "qtk", "qttqk", "mm"];
    let tiled = assert_sound(&g, &words, &words);
    let tiles = &tiled.evidence().tiles;
    assert_eq!((tiles.repeating, tiles.not_tileable), (1, 0));
    assert_rejected(&g, &tiled, &["kk", "qtqk", "k"]);
}
