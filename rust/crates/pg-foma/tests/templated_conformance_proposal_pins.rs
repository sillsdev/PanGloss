//! Focused templated conformance proposal pins.

use pg_conformance_fixtures::{assert_matches_oracle, discover, FixtureRef, Root, WordEntry};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::scoreboard::{self, CellOutcome};
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

/// `eGuessPat`'s `[Any]*` now compiles (see `pattern_root_token_route_gate.rs`); this checks the unrelated raised-surface derivation still proposes correctly on the same, unstripped grammar.
#[test]
fn polysynthetic_stratal_derivation_chain_admits_pattern_and_proposes_raised_surface() {
    let (label, grammar, words) = open(
        Root::Machine,
        "languages",
        "polysynthetic-stratal-derivation-chain",
    );

    compile_templated_morphotactics(&grammar)
        .unwrap_or_else(|e| panic!("{label}: eGuessPat's pattern root must now compile: {e}"));

    assert!(
        word(&words, "kuiikuii").expect_fail,
        "{label}: raw \"kuiikuii\" must remain the committed negative case"
    );
    assert_proposes_oracle_identity(&label, &grammar, &words, "kuuukuuu");
}

fn assert_proposes_all_oracle_identities(
    label: &str,
    grammar: &Grammar,
    words: &pg_conformance_fixtures::WordsYaml,
    surface: &str,
) {
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
    assert!(
        !outcome.structured.is_empty(),
        "{label}: expected at least one oracle identity for {surface:?}"
    );

    let mut compiled = compile_templated_morphotactics(grammar)
        .unwrap_or_else(|e| panic!("{label}: templated compile failed: {e}"));
    let candidates = compiled.proposer.propose(surface);

    for analysis in &outcome.structured {
        let expected: (Vec<MorphemeId>, i32) = (
            analysis
                .morpheme_ids
                .iter()
                .copied()
                .map(MorphemeId)
                .collect(),
            analysis.root_morpheme_index,
        );
        assert!(
            candidates
                .iter()
                .any(|candidate| (candidate.morphemes.clone(), candidate.root_index) == expected),
            "{label}: templated proposer must contain oracle identity for {surface:?}; expected \
             {:?}, got {:?}",
            expected,
            candidates
        );
    }
}

/// `mrSetX`/`mrSetY` are Unordered siblings, so both relative orders ("daboxayu"/"daboyuxa") must propose.
#[test]
fn mpr_overwrite_order_dependence_proposes_both_relative_orders() {
    let (label, grammar, words) = open(
        Root::Machine,
        "edge-cases",
        "mpr-overwrite-order-dependence",
    );
    assert_proposes_oracle_identity(&label, &grammar, &words, "daboxayu");
    assert_proposes_oracle_identity(&label, &grammar, &words, "daboyuxa");
}

/// `rulePfx`/`ruleObj` are Unordered siblings, so "imat" must propose all 3 oracle identities (either stacking order, or `ruleObj` alone).
#[test]
fn strrep_identity_proposes_every_stacking_order() {
    let (label, grammar, words) = open(Root::Machine, "edge-cases", "strrep-identity");
    assert_proposes_all_oracle_identities(&label, &grammar, &words, "imat");
    assert_proposes_all_oracle_identities(&label, &grammar, &words, "ndpat");
}

/// `prAlpha` flips featHigh on the output vowel (polarity="minus"); "isk" needs the disagree-polarity resolution `resolve_alpha_tuples` now implements.
#[test]
fn feature_system_breadth_proposes_alpha_polarity_flip() {
    let (label, grammar, words) = open(Root::Machine, "edge-cases", "feature-system-breadth");
    assert_proposes_oracle_identity(&label, &grammar, &words, "isk");
}

/// `prDoubleAlpha`'s ambiguous two-feature disagreement must stay an honest refusal, never a silent miscompile.
#[test]
fn alpha_variable_name_collision_stays_an_honest_refusal() {
    let (label, grammar, _words) = open(Root::Machine, "edge-cases", "alpha-variable-name-collision");
    match compile_templated_morphotactics(&grammar) {
        Err(_) => {}
        Ok(_) => panic!(
            "{label}: an ambiguous disagree-polarity alpha rule must not silently compile"
        ),
    }
}

#[test]
fn truncate_morphotactic_proposes_successful_truncation_controls() {
    let (label, grammar, words) = open(Root::Machine, "edge-cases", "truncate-morphotactic");

    assert_proposes_oracle_identity(&label, &grammar, &words, "sa");
    assert_proposes_oracle_identity(&label, &grammar, &words, "ag");
    assert_proposes_oracle_identity(&label, &grammar, &words, "as");
    assert_proposes_oracle_identity(&label, &grammar, &words, "gbubibi");
}

/// "gas" has a direct one-rule oracle identity and a chained two-rule one; the templated route proposes both.
#[test]
fn truncate_morphotactic_proposes_both_gas_analyses() {
    let (label, grammar, words) = open(Root::Machine, "edge-cases", "truncate-morphotactic");
    let entry = word(&words, "gas");
    let morpher = Morpher::new(&grammar, usize::MAX);
    let outcome = morpher.parse_word_opts("gas", &ParseOptions::default());
    assert!(
        !outcome.invalid_shape,
        "{label}: \"gas\" unexpectedly has invalid shape"
    );
    assert_eq!(
        outcome.signature(),
        entry.expected_signature(),
        "{label}: oracle drift for \"gas\""
    );
    assert_eq!(
        outcome.structured.len(),
        2,
        "{label}: this focused pin requires both oracle identities for \"gas\""
    );

    let mut compiled = compile_templated_morphotactics(&grammar)
        .unwrap_or_else(|e| panic!("{label}: templated compile failed: {e}"));
    let candidates = compiled.proposer.propose("gas");
    // `WordAnalysis` has no rule-name field, so the two analyses are told apart by morpheme count.
    let identity_of = |morpheme_count: usize| {
        outcome
            .structured
            .iter()
            .find(|analysis| analysis.morpheme_ids.len() == morpheme_count)
            .unwrap_or_else(|| {
                panic!("{label}: oracle must report a {morpheme_count}-morpheme analysis for \"gas\"")
            })
    };
    let direct = identity_of(2);
    let expected_direct = (
        direct
            .morpheme_ids
            .iter()
            .copied()
            .map(MorphemeId)
            .collect::<Vec<_>>(),
        direct.root_morpheme_index,
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| (candidate.morphemes.clone(), candidate.root_index)
                == expected_direct),
        "{label}: templated proposer must contain the direct oracle identity {expected_direct:?} \
         for \"gas\"; got {candidates:?}"
    );

    let chained = identity_of(3);
    let expected_chained = (
        chained
            .morpheme_ids
            .iter()
            .copied()
            .map(MorphemeId)
            .collect::<Vec<_>>(),
        chained.root_morpheme_index,
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| (candidate.morphemes.clone(), candidate.root_index)
                == expected_chained),
        "{label}: templated proposer must contain the chained oracle identity {expected_chained:?} \
         for \"gas\" (the order lattice reaches the truncating rule after the inserting one); got \
         {candidates:?}"
    );
}

/// Pins this fixture's scoreboard cell at `OracleExact` under the templated route (see the pin above).
#[test]
fn truncate_morphotactic_scoreboard_cell_under_templated_underlying_tokens() {
    let (label, grammar, words) = open(Root::Machine, "edge-cases", "truncate-morphotactic");
    let words: Vec<String> = words
        .words
        .iter()
        .filter(|entry| !entry.expect_fail)
        .map(|entry| entry.word.clone())
        .collect();
    let scored = scoreboard::measure(&label, &grammar, &words);
    let cell = scored
        .cells
        .iter()
        .find(|cell| cell.strategy == EmissionStrategy::TemplatedUnderlyingTokens)
        .expect("TemplatedUnderlyingTokens must have a scoreboard cell");
    assert_eq!(
        cell.outcome,
        CellOutcome::OracleExact,
        "{label} [TemplatedUnderlyingTokens]: got {:?} (cert={}) -- both \"gas\" analyses were \
         reachable once the leading drop anchored to its marker and the order lattice landed; \
         anything else is a regression",
        cell.outcome,
        cell.certification_debug
    );
}

