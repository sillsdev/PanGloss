//! The parity relation, exercised at the certification seam.
//!
//! These used to run against hand-built `WordAnalysis` values with no grammar at all, because
//! full structural equality needs no model. Deduplicated identity comparison DOES need one --
//! the whole point is that dense ordinals are projected to stable source keys -- so they now
//! compile `test_support::PARITY_FIXTURE_XML`, three unrelated entries whose only job is to give
//! three morpheme ordinals something to resolve to.

use super::*;
use crate::test_support::{parity_analysis, parity_fixture_grammar};

fn wa(n: u32) -> WordAnalysis {
    parity_analysis(n)
}

/// Same identity as `wa(n)`, differing only in `mpr` — a field `AnalysisIdentity` does not capture and `WordAnalysis::Eq` does.
fn wa_other_mpr(n: u32) -> WordAnalysis {
    WordAnalysis {
        mpr: pg_grammar::model::MprSet(1),
        ..wa(n)
    }
}

fn fixture() -> Grammar {
    parity_fixture_grammar()
}

#[test]
fn order_is_irrelevant() {
    let g = fixture();
    assert!(certify_word(&g, "w", &[wa(0), wa(1)], &[wa(1), wa(0)]).selectable());
}

#[test]
fn analyses_differing_only_outside_identity_are_the_same_analysis() {
    // wa(0) and wa_other_mpr(0) are unequal as WordAnalysis values but must certify: AnalysisIdentity captures only stable morpheme keys, root position, and category — mpr is engine-internal payload, not identity.
    let g = fixture();
    assert_ne!(wa(0), wa_other_mpr(0), "the fixture must actually differ");
    let verdict = certify_word(&g, "w", &[wa(0)], &[wa_other_mpr(0)]);
    assert!(
        verdict.selectable(),
        "identity-invisible payload must not read as disagreement: {verdict:?}"
    );
}

#[test]
fn genuinely_different_identities_still_disagree() {
    // Widening the equivalence must not make everything equal: different morphemes are different identities.
    let g = fixture();
    // Neither side's identity is a subset of the other, so the mismatch disagrees in BOTH directions -- not a bare `{ .. }` match, since the whole point of `direction` is that a caller can read it structurally.
    assert!(matches!(
        certify_word(&g, "w", &[wa(0)], &[wa(1)]),
        Certification::IdentityMismatch {
            direction: IdentityMismatchDirection::Both,
            ..
        }
    ));
    // ... and so is the same morpheme sequence at a different root position.
    let original = WordAnalysis {
        morpheme_ids: vec![0, 1],
        root_morpheme_index: 0,
        morpheme_roots: vec![None, None],
        ..wa(0)
    };
    let moved = WordAnalysis {
        root_morpheme_index: 1,
        ..original.clone()
    };
    assert!(matches!(
        certify_word(&g, "w", &[original], &[moved]),
        Certification::IdentityMismatch {
            direction: IdentityMismatchDirection::Both,
            ..
        }
    ));
}

#[test]
fn identity_mismatch_direction_separates_recall_miss_from_over_generation_at_the_certify_seam() {
    // The three sites above are all symmetric (Both) mismatches; this pins the asymmetric cases through the SAME public entry point `faithfulness_coverage`/gates actually call.
    let g = fixture();
    // Candidate is missing `wa(0)` and offers nothing extra: a pure recall miss (ADR-0001's "never miss").
    assert!(matches!(
        certify_word(&g, "w", &[wa(0), wa(1)], &[wa(1)]),
        Certification::IdentityMismatch {
            direction: IdentityMismatchDirection::RecallMiss,
            ..
        }
    ));
    // Candidate finds everything the oracle found, plus `wa(2)` the oracle never licensed: a pure surviving over-generation -- the soundness hazard this task exists to gate.
    assert!(matches!(
        certify_word(&g, "w", &[wa(1)], &[wa(1), wa(2)]),
        Certification::IdentityMismatch {
            direction: IdentityMismatchDirection::OverGeneration,
            ..
        }
    ));
}

#[test]
fn duplicate_paths_collapse_without_changing_the_verdict() {
    // The parity relation is deduplicated identity SET equality: an oracle reaching one analysis by two derivational paths and a candidate reaching it by one have found the same set and agree — multiplicity is evidence about redundant proposal work, never a grammar difference.
    let g = fixture();
    assert!(certify_word(&g, "w", &[wa(0), wa(0)], &[wa(0)]).selectable());
    assert!(certify_word(&g, "w", &[wa(0)], &[wa(0), wa(0), wa(0)]).selectable());
}

#[test]
fn collapsed_duplicate_paths_survive_as_evidence() {
    // The verdict ignores duplicate paths, but the evidence must not lose them, or "these agree" would be indistinguishable from "these agree and one side did three times the work".
    let g = fixture();
    let projected =
        crate::parity::OccurrenceIdentities::project(&[wa(0), wa(0), wa(0)], &g).unwrap();
    assert_eq!(projected.len(), 1);
    assert_eq!(projected.entries()[0].duplicate_paths, 3);
    assert_eq!(projected.collapsed_paths(), 2);
}

#[test]
fn repeated_corpus_rows_stay_separate_observations() {
    // Deduplication is within an occurrence: two rows for the same word are two observations, and a candidate disagreeing on the second row fails even though the union of both rows' identities would match.
    let g = fixture();
    let expected = vec![("w".into(), vec![wa(0)]), ("w".into(), vec![wa(1)])];
    assert!(certify_corpus(&g, &expected, &expected).selectable());
    let changed = vec![("w".into(), vec![wa(0)]), ("w".into(), vec![wa(0)])];
    assert!(matches!(
        certify_corpus(&g, &expected, &changed),
        Certification::IdentityMismatch {
            direction: IdentityMismatchDirection::Both,
            ..
        }
    ));
    // Collapsing the two rows into one would also change the row count, which is refused outright rather than certified against a shorter corpus.
    let collapsed = vec![("w".to_string(), vec![wa(0), wa(1)])];
    assert!(matches!(
        certify_corpus(&g, &expected, &collapsed),
        Certification::Truncated { .. }
    ));
}

#[test]
fn a_projection_failure_is_a_typed_fault_never_a_mismatch_and_never_a_pass() {
    // An analysis referencing a morpheme its own model lacks is an internal inconsistency: reporting it as a mismatch would blame the grammar for an engine bug, and — more dangerous — the two sides here are identical, so a lazy or missing projection would call this a full confirmation.
    let g = fixture();
    let unresolvable = wa(9_999);
    let verdict = certify_word(
        &g,
        "w",
        std::slice::from_ref(&unresolvable),
        std::slice::from_ref(&unresolvable),
    );
    assert!(
        !verdict.selectable(),
        "an unprojectable analysis must never certify: {verdict:?}"
    );
    assert!(
        matches!(&verdict, Certification::Truncated { stage, .. }
            if stage == "identity-projection-failed-oracle"),
        "expected a typed projection truncation naming its side, got {verdict:?}"
    );
    // A candidate-side-only fault is named as such, so a report can tell which engine is broken.
    let candidate_side = certify_word(&g, "w", &[wa(0)], std::slice::from_ref(&unresolvable));
    assert!(
        matches!(&candidate_side, Certification::Truncated { stage, .. }
            if stage == "identity-projection-failed-candidate"),
        "got {candidate_side:?}"
    );
}

#[test]
fn guessing_is_refused_by_the_v1_certification_scope() {
    let g = fixture();
    let guessed = WordAnalysis {
        guessed: true,
        ..wa(0)
    };
    // Identical on both sides, so a comparison that only compared would confirm.
    let verdict = certify_word(
        &g,
        "w",
        std::slice::from_ref(&guessed),
        std::slice::from_ref(&guessed),
    );
    assert!(
        matches!(&verdict, Certification::Truncated { stage, .. }
            if stage == "guessing-refused-oracle"),
        "a guessed analysis must be refused, not certified: {verdict:?}"
    );
    let by_provenance = WordAnalysis {
        provenance: pg_parse::AnalysisProvenance::Guessed,
        ..wa(0)
    };
    assert!(
        matches!(
            certify_word(&g, "w", &[wa(0)], std::slice::from_ref(&by_provenance)),
            Certification::Truncated { ref stage, .. } if stage == "guessing-refused-candidate"
        ),
        "the provenance tag must be refused on its own, not only the boolean"
    );
}

#[test]
fn supplied_roots_are_refused_by_the_v1_certification_scope() {
    let g = fixture();
    let supplied = WordAnalysis {
        provenance: pg_parse::AnalysisProvenance::Supplied {
            entry_id: "runtime-entry".into(),
        },
        ..wa(0)
    };
    let verdict = certify_word(
        &g,
        "w",
        std::slice::from_ref(&supplied),
        std::slice::from_ref(&supplied),
    );
    assert!(
        matches!(&verdict, Certification::Truncated { stage, .. }
            if stage == "supplied-root-refused-oracle"),
        "a supplied root must be refused, not certified: {verdict:?}"
    );
}

#[test]
fn a_total_lexical_miss_reaches_the_oracle_as_a_miss_not_a_guess() {
    // PreparedCorpus::prepare reads its oracle with guess_root: false and no SuppliedRootOverlay, so a word the lexicon cannot reach comes back with zero analyses rather than a fabricated root; this is a preservation guard, not the v1 scope gate, which the refusal tests above enforce.
    let g = fixture();
    let miss = vec!["cb".to_string()];
    let prepared = PreparedCorpus::prepare(&g, &miss, RuntimeBudget::default())
        .expect("preparation must not trip the liveness net on a one-word corpus");
    let selection = prepared.select(&miss);
    assert!(
        selection.exclusions.is_empty(),
        "an unanalyzable word is comparable, not excluded: {:?}",
        selection.exclusions
    );
    let analyses = &selection.expected[0].1;
    assert!(
        analyses.iter().all(|a| !a.guessed
            && a.supplied_root.is_none()
            && matches!(a.provenance, pg_parse::AnalysisProvenance::Grammar)),
        "the oracle must produce grammar-provenance analyses only: {analyses:?}"
    );
}

#[test]
fn a_corpus_of_only_unanalyzable_words_is_still_refused() {
    // The vacuous-pass guard: an empty set equals an empty set under set equality too, which is exactly why this guard exists.
    let g = fixture();
    assert!(matches!(
        certify_corpus(&g, &[("w".into(), vec![])], &[("w".into(), vec![])]),
        Certification::Truncated { ref stage, .. } if stage == "no-analyzable-words"
    ));
}

#[test]
fn an_honest_all_negative_fixture_certifies_when_a_real_backend_ran() {
    // Same all-empty shape as the guard above, but a real backend evaluated the word.
    let g = fixture();
    let (verdict, _) = certify_corpus_with_execution_evidence(
        &g,
        &[("w".into(), vec![])],
        &[("w".into(), vec![])],
        true,
    );
    assert!(
        verdict.selectable(),
        "an all-negative fixture a real backend evaluated must certify: {verdict:?}"
    );
}

#[test]
fn missing_truncated() {
    let g = fixture();
    assert!(matches!(
        certify_corpus(&g, &[("w".into(), vec![])], &[]),
        Certification::Truncated { .. }
    ));
}

fn evidence_for_containment(
    expected: Vec<WordAnalysis>,
    proposals: Vec<crate::tags::Candidate>,
) -> WordEvidence {
    WordEvidence {
        word: "w".to_string(),
        expected,
        actual: Vec::new(),
        proposals,
        expected_identities: None,
        actual_identities: None,
    }
}

fn candidate(morphemes: &[u32], root_index: i32) -> crate::tags::Candidate {
    crate::tags::Candidate {
        morphemes: morphemes
            .iter()
            .map(|&m| pg_grammar::model::MorphemeId(m))
            .collect(),
        root_index,
    }
}

/// Presence, not multiplicity, is what `word_proposal_containment` checks -- see its own doc.
#[test]
fn an_identity_required_twice_but_proposed_once_still_holds_containment() {
    let evidence = evidence_for_containment(
        vec![wa(0), wa(0)],
        vec![candidate(&[0], wa(0).root_morpheme_index)],
    );
    assert_eq!(
        word_proposal_containment(&evidence),
        Ok(()),
        "presence, not multiplicity, is the pre-confirm containment question"
    );
}

/// A dropped identity (offered zero times) must still fail containment.
#[test]
fn an_identity_never_proposed_at_all_still_fails_containment() {
    let evidence = evidence_for_containment(vec![wa(0), wa(1)], vec![candidate(&[0], 0)]);
    let gap = word_proposal_containment(&evidence)
        .expect_err("a dropped identity must still fail containment");
    assert_eq!(gap.morpheme_ids, vec![1]);
}

/// `selectable()` answers accuracy AND publishability; only their conjunction can make it true.
#[test]
fn selectable_requires_both_accurate_certification_and_clean_production_health() {
    let confirmed = Certification::FullHcConfirmed {
        words: 1,
        corpus_hash: "h".into(),
    };
    let score = Score {
        states: 1,
        arcs: 1,
        build: 1,
        apply: 1,
        proposals: 1,
        confirmation: 1,
        confirmation_steps: 1,
        raw_paths: 0,
    };
    let divergence = IdentityDivergence::not_compared(0);
    let clean_health = HealthReport::new(vec![]);
    let blocked_health = unassessed_production_health("test block".into());

    let evaluation =
        |certification: Certification, production_health: HealthReport| RuntimeEvaluation {
            certification,
            score,
            realized_strategy: EmissionStrategy::PlanComposed,
            divergence,
            production_health,
        };

    assert!(
        evaluation(confirmed.clone(), clean_health.clone()).selectable(),
        "accurate certification + clean production health must select"
    );
    assert!(
        !evaluation(Certification::EstimateOnly, clean_health.clone()).selectable(),
        "an unselectable certification alone must refuse, even with clean production health"
    );
    assert!(
        !evaluation(confirmed.clone(), blocked_health.clone()).selectable(),
        "blocked production health alone must refuse, even with an accurate certification"
    );
    assert!(
        !evaluation(Certification::EstimateOnly, blocked_health).selectable(),
        "both halves failing must refuse"
    );
}
