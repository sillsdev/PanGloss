//! Each staged ad hoc prohibition case carries a LibLCM-authored `fieldworks/project.fwdata`; importing it and compiling it must give the analysis counts `words.yaml` records from C# `hc.dll` on the case's `grammar.xml`, because the FieldWorks compile step is where the ad hoc prohibition copies were lost (#99, #100).

use pg_conformance_fixtures::require_fixture;
use pg_parse::{Morpher, ParseOptions};

/// Imports the case's FieldWorks witness and returns `(word, analysis count, expected count)` for every word in `words.yaml`.
fn witness_counts(case: &str) -> Vec<(String, usize, usize)> {
    let fixture = require_fixture("edge-cases", case);
    let project = fixture.dir.join("fieldworks").join("project.fwdata");
    assert!(project.is_file(), "{case}: missing {}", project.display());
    let (snapshot, _report) = pg_fwdata::import_file(&project).unwrap();
    let (grammar, _warnings) = pg_grammar::compile_project(&snapshot).unwrap();
    let morpher = Morpher::new(&grammar, usize::MAX);
    fixture
        .load_words_yaml()
        .words
        .iter()
        .map(|entry| {
            let out = morpher.parse_word_opts(&entry.word, &ParseOptions::default());
            assert!(
                !out.invalid_shape && !out.capped && !out.timed_out && !out.guessed,
                "{case}/{}: parse did not complete cleanly",
                entry.word
            );
            (entry.word.clone(), out.structured.len(), entry.parses.len())
        })
        .collect()
}

fn assert_witness_agrees(case: &str) {
    let counts = witness_counts(case);
    assert!(
        counts.iter().any(|(_, _, expected)| *expected == 0)
            && counts.iter().any(|(_, _, expected)| *expected > 0),
        "{case}: a witness needs both prohibited words and controls"
    );
    let wrong: Vec<String> = counts
        .iter()
        .filter(|(_, got, expected)| got != expected)
        .map(|(word, got, expected)| format!("{word}: {got} analyses, C# has {expected}"))
        .collect();
    assert!(
        wrong.is_empty(),
        "{case}: the FieldWorks project disagrees with grammar.xml on {}",
        wrong.join("; ")
    );
}

#[test]
fn fieldworks_witness_of_stem_msa_copies_matches_the_oracle() {
    assert_witness_agrees("adhoc-prohibition-stem-msa-copies");
}

#[test]
fn fieldworks_witness_of_affix_msa_copies_matches_the_oracle() {
    assert_witness_agrees("adhoc-prohibition-affix-msa-copies");
}

#[test]
fn fieldworks_witness_of_variant_msa_copies_matches_the_oracle() {
    assert_witness_agrees("adhoc-prohibition-variant-msa-copies");
}

#[test]
fn fieldworks_witness_of_affix_environment_copies_matches_the_oracle() {
    assert_witness_agrees("adhoc-prohibition-affix-environment-copies");
}

#[test]
fn fieldworks_witness_of_circumfix_pairings_matches_the_oracle() {
    assert_witness_agrees("adhoc-prohibition-circumfix-pairings");
}
