//! Differential measurement for `crate::build::unbuildable_marker_material` over six fixtures whose plan carries a composite/structural-composite marker leaf.
use pg_conformance_fixtures::discover;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::scoreboard::{self, CellOutcome};

/// `(fixture label, expected PlanComposed outcome, cause)` -- the `cause` column carries the detail; see `crate::build::unbuildable_marker_material`/`crate::emit::marker_admission_is_complete`'s own docs for the general rule each row instantiates.
const FIXTURES: &[(&str, &str, &str)] = &[
    (
        "machine:edge-cases/mpr-gated-exception",
        "refused",
        "empty marker material; admitting would expose an unrelated, pre-existing gap (mentanukam: MorphologicalOutput.MPRFeatures->PhonologicalSubrule.excludedMPRFeatures, an affix-conferred phonological-rule gate build_controllable's static gate/replace cascade cannot model)",
    ),
    (
        "machine:edge-cases/right-to-left-anchor-environment",
        "refused",
        "empty marker material (no morphology at all); admission would gain nothing so is never attempted",
    ),
    (
        "machine:edge-cases/loader-default-symbol",
        "refused",
        "empty marker material; admitting would expose an unrelated, pre-existing gap (bat: SymbolicFeature@defaultSymbol/UseDefaults not honoured by the compiled rewrite-rule cascade)",
    ),
    (
        "machine:edge-cases/truncate-morphotactic",
        "oracle_exact",
        "non-empty structural-composite material; every rule is structural-candidate-claimed and the grammar has no template, so marker_admission_is_complete holds",
    ),
    (
        "machine:edge-cases/process-morphology-in-place-mutation",
        "oracle_exact",
        "non-empty structural-composite material (mrAblaut, Role::Process); the grammar has no other rule and no template, so marker_admission_is_complete holds",
    ),
    (
        "staging:edge-cases/circumfix-in-template-slot",
        "refused",
        "non-empty structural-composite material, but the grammar declares an AffixTemplate, so marker_admission_is_complete's conservative template check refuses it even though this specific slot's only other occupant is harmless",
    ),
];

#[test]
fn plan_composed_marker_material_outcomes_match_the_ratchet() {
    let fixtures = discover();
    let mut missing: Vec<&str> = Vec::new();
    let mut mismatches: Vec<String> = Vec::new();

    for &(label, expected, why) in FIXTURES {
        let Some(fixture) = fixtures.iter().find(|f| f.label() == label) else {
            missing.push(label);
            continue;
        };
        let words_yaml = fixture.load_words_yaml();
        assert!(
            !words_yaml.expect_crash,
            "{label}: fixture is expect_crash-excluded, cannot be measured by this gate"
        );
        let grammar = pg_grammar::load(&fixture.load_grammar_xml())
            .unwrap_or_else(|e| panic!("{label}: grammar failed to load: {e}"));
        let words: Vec<String> = words_yaml.words.iter().map(|w| w.word.clone()).collect();
        let scored = scoreboard::measure(label, &grammar, &words);
        let cell = scored
            .cells
            .iter()
            .find(|c| c.strategy == EmissionStrategy::PlanComposed)
            .unwrap_or_else(|| panic!("{label}: no PlanComposed cell in measurement"));

        let got = match &cell.outcome {
            CellOutcome::OracleExact => "oracle_exact",
            CellOutcome::CompilesButMisses { .. } => "compiles_but_misses",
            CellOutcome::Refused { .. } => "refused",
            CellOutcome::Unmeasurable { .. } => "unmeasurable",
        };
        if got != expected {
            mismatches.push(format!(
                "{label}: expected {expected} but measured {got} ({:?}); documented cause: {why}",
                cell.outcome
            ));
        }

        if let Some(divergence) = cell.divergence {
            assert_eq!(
                divergence.candidate_only_identities, 0,
                "{label}: PlanComposed proposed an identity the oracle does not have -- a soundness \
                 violation, never acceptable regardless of this gate's own pass/fail"
            );
        }
    }

    assert!(
        missing.is_empty(),
        "fixture(s) not discovered (check PANGLOSS_CONFORMANCE_SCOPE / label spelling): {missing:?}"
    );
    assert!(
        mismatches.is_empty(),
        "PlanComposed outcome drifted from this ratchet's own pinned expectations:\n{}",
        mismatches.join("\n")
    );
}
