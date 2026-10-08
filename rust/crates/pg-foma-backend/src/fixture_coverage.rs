//! Shared passing-fixture evidence for the coverage gate and CLI.

use std::collections::HashSet;

use pg_conformance_fixtures::{FixtureRef, WordEntry, WordsYaml};
use pg_foma::capability::{default_registry, observed_variants};
use pg_grammar::model::Grammar;
use pg_parse::Morpher;
use serde::Serialize;

/// Local tag extensions for immutable upstream fixtures, equivalent to per-word variant tags.
/// Each extension still requires a passing original construct tag and the owning predicate's
/// structural witness; it cannot credit a fixture merely because its name is listed.
pub const UPSTREAM_VARIANT_TAG_EXTENSIONS: &[(&str, pg_foma::capability::ConstructVariant)] = &[
    (
        "machine:edge-cases/iterative-epenthesis-cascade",
        pg_foma::capability::ConstructVariant::EpenthesisStructural,
    ),
    (
        "machine:edge-cases/simultaneous-feeding",
        pg_foma::capability::ConstructVariant::SimultaneousDisjoint,
    ),
    (
        "machine:edge-cases/mpr-group-overwrite-without-realizational",
        pg_foma::capability::ConstructVariant::MprOverwrite,
    ),
];

/// A grammar that supplied no evidence because reading or loading it failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FixtureLoadFailure {
    pub fixture: String,
    pub error: String,
}

/// Coverage evidence and the failures that prevented evidence from being collected.
#[derive(Debug, Default)]
pub struct FixtureCoverage {
    pub passing_constructs: HashSet<String>,
    pub load_failures: Vec<FixtureLoadFailure>,
    pub invalid_variant_tags: Vec<String>,
}

/// The shared adapter replay decision. Skipped fixtures, invisible words, expected or actual
/// skips, and signature mismatches cannot witness a construct.
pub fn word_passes_for_coverage(words: &WordsYaml, word: &WordEntry, morpher: &Morpher) -> bool {
    if words.skip_in_generic_replay().is_some() || !word.adapter_visible() || word.expect_skip {
        return false;
    }
    let outcome = morpher.parse_word(&word.word);
    !outcome.invalid_shape && outcome.signature() == word.expected_signature()
}

/// Replays tags against the loaded grammar. A concrete variant tag must also be observed by
/// its owning capability predicate; a generic kind tag never substitutes for that tag.
pub fn coverage_for_grammar(label: &str, grammar: &Grammar, words: &WordsYaml) -> FixtureCoverage {
    let registry = default_registry();
    let variants = observed_variants(grammar, &registry);
    let known: HashSet<_> = registry
        .predicates()
        .iter()
        .flat_map(|p| p.variants().iter().map(|v| v.id()))
        .collect();
    let observed: HashSet<_> = variants.iter().map(|v| v.id()).collect();
    let morpher = Morpher::new(grammar, usize::MAX);
    let mut report = FixtureCoverage::default();
    for word in &words.words {
        let tags: Vec<_> = word
            .exercises
            .iter()
            .chain(word.parses.iter().flat_map(|p| &p.exercises))
            .collect();
        let passing = word_passes_for_coverage(words, word, &morpher);
        for tag in &tags {
            if known.contains(tag.as_str()) && !observed.contains(tag.as_str()) {
                report.invalid_variant_tags.push(format!(
                    "{label} word {:?}: variant {tag} is absent from the loaded grammar",
                    word.word
                ));
            } else if passing {
                report.passing_constructs.insert((*tag).clone());
            }
        }
        for &(fixture_label, variant) in UPSTREAM_VARIANT_TAG_EXTENSIONS {
            if label == fixture_label
                && tags.iter().any(|tag| {
                    crate::conformance_coverage::construct_ids_for(variant.kind())
                        .contains(&tag.as_str())
                })
            {
                if !variants.contains(&variant) {
                    report.invalid_variant_tags.push(format!(
                        "{label}: upstream tag extension {} is absent from the loaded grammar",
                        variant.id()
                    ));
                } else if passing {
                    report.passing_constructs.insert(variant.id().to_string());
                }
            }
        }
    }
    report
}

/// Collects passing evidence while retaining named grammar load failures for both consumers.
/// Failed loads contribute no tags and never disappear as successful evidence.
pub fn passing_covered_constructs(fixtures: &[FixtureRef]) -> FixtureCoverage {
    let mut report = FixtureCoverage::default();
    for fixture in fixtures {
        let words = fixture.load_words_yaml();
        if words.skip_in_generic_replay().is_some() {
            continue;
        }
        let grammar = std::fs::read_to_string(fixture.grammar_path())
            .map_err(|e| e.to_string())
            .and_then(|xml| pg_grammar::load(&xml).map_err(|e| e.to_string()));
        match grammar {
            Ok(grammar) => {
                let evidence = coverage_for_grammar(&fixture.label(), &grammar, &words);
                report
                    .passing_constructs
                    .extend(evidence.passing_constructs);
                report
                    .invalid_variant_tags
                    .extend(evidence.invalid_variant_tags);
            }
            Err(error) => report.load_failures.push(FixtureLoadFailure {
                fixture: fixture.label(),
                error,
            }),
        }
    }
    report
}

#[cfg(test)]
mod tests;
