//! Focused templated conformance proposal pins.

use pg_conformance_fixtures::{assert_matches_oracle, discover, FixtureRef, Root, WordEntry};
use pg_foma::templated_compile::compile_templated_morphotactics;
use pg_grammar::model::{Grammar, MorphemeId};
use pg_parse::{Morpher, ParseOptions};

fn fixture(root: Root, category: &str, name: &str) -> FixtureRef {
    discover()
        .into_iter()
        .find(|f| f.root == root && f.category == category && f.name == name)
        .unwrap_or_else(|| panic!("missing conformance fixture {root:?}:{category}/{name}"))
}

fn open(
    root: Root,
    category: &str,
    name: &str,
) -> (String, Grammar, pg_conformance_fixtures::WordsYaml) {
    let fixture = fixture(root, category, name);
    let label = fixture.label();
    let grammar = pg_grammar::load(&fixture.load_grammar_xml())
        .unwrap_or_else(|e| panic!("{label}: fixture failed to load: {e}"));
    (label, grammar, fixture.load_words_yaml())
}

fn word<'a>(words: &'a pg_conformance_fixtures::WordsYaml, surface: &str) -> &'a WordEntry {
    words
        .words
        .iter()
        .find(|entry| entry.word == surface)
        .unwrap_or_else(|| panic!("words.yaml is missing {surface:?}"))
}

fn oracle_identity(
    label: &str,
    grammar: &Grammar,
    words: &pg_conformance_fixtures::WordsYaml,
    surface: &str,
) -> (Vec<MorphemeId>, i32) {
    let entry = word(words, surface);
    let morpher = Morpher::new(grammar, usize::MAX);
    let outcome = morpher.parse_word_opts(surface, &ParseOptions::default());
    assert!(
        !outcome.invalid_shape,
        "{label}: {surface:?} unexpectedly has invalid shape"
    );
    assert_eq!(
        outcome.signature(),
        entry.expected_signature(),
        "{label}: oracle drift for {surface:?}"
    );
    assert_eq!(
        outcome.structured.len(),
        1,
        "{label}: this focused pin requires one oracle identity for {surface:?}"
    );
    let analysis = &outcome.structured[0];
    (
        analysis
            .morpheme_ids
            .iter()
            .copied()
            .map(MorphemeId)
            .collect(),
        analysis.root_morpheme_index,
    )
}

fn assert_proposes_oracle_identity(
    label: &str,
    grammar: &Grammar,
    words: &pg_conformance_fixtures::WordsYaml,
    surface: &str,
) {
    let expected = oracle_identity(label, grammar, words, surface);
    let mut compiled = compile_templated_morphotactics(grammar)
        .unwrap_or_else(|e| panic!("{label}: templated compile failed: {e}"));
    let candidates = compiled.proposer.propose(surface);
    assert!(
        candidates
            .iter()
            .any(|candidate| (candidate.morphemes.clone(), candidate.root_index) == expected),
        "{label}: templated proposer must contain oracle identity for {surface:?}; expected {:?}, got {:?}",
        expected,
        candidates
    );
}

#[test]
fn loader_default_symbol_proposes_bat_root_and_keeps_bdt_rule_control_analyzable() {
    let (label, grammar, words) = open(Root::Machine, "edge-cases", "loader-default-symbol");
    assert_eq!(
        assert_matches_oracle(&label, &words, &Morpher::new(&grammar, usize::MAX)),
        words.words.len()
    );

    assert_proposes_oracle_identity(&label, &grammar, &words, "bat");

    let mut compiled = compile_templated_morphotactics(&grammar)
        .unwrap_or_else(|e| panic!("{label}: templated compile failed: {e}"));
    let bdt_candidates = compiled.proposer.propose("bdt");
    assert!(
        !bdt_candidates.is_empty(),
        "{label}: positive-control rule case \"bdt\" must remain analyzable by the proposer"
    );
}

#[test]
fn subrule_morphosyntactic_gating_proposes_bare_and_derived_identities() {
    let (label, grammar, words) = open(
        Root::Machine,
        "edge-cases",
        "subrule-morphosyntactic-gating",
    );
    assert_eq!(
        assert_matches_oracle(&label, &words, &Morpher::new(&grammar, usize::MAX)),
        words.words.len()
    );

    assert_proposes_oracle_identity(&label, &grammar, &words, "pat");
    assert_proposes_oracle_identity(&label, &grammar, &words, "bat");
}

#[test]
fn polysynthetic_stratal_derivation_chain_proposes_ms_identity_only_for_raised_surface() {
    let (label, grammar, words) = open(
        Root::Machine,
        "languages",
        "polysynthetic-stratal-derivation-chain",
    );

    assert!(
        word(&words, "kuiikuii").expect_fail,
        "{label}: raw \"kuiikuii\" must remain the committed negative case"
    );
    assert_proposes_oracle_identity(&label, &grammar, &words, "kuuukuuu");
}

#[test]
fn truncate_morphotactic_proposes_successful_truncation_controls() {
    let (label, grammar, words) = open(Root::Machine, "edge-cases", "truncate-morphotactic");

    assert_proposes_oracle_identity(&label, &grammar, &words, "sa");
    assert_proposes_oracle_identity(&label, &grammar, &words, "ag");
    assert_proposes_oracle_identity(&label, &grammar, &words, "as");
}
