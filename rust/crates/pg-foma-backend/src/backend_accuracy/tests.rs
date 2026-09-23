use super::*;
use pg_grammar::model::MorphemeId;

fn analysis(morphemes: &[u32], root_index: i32) -> WordAnalysis {
    WordAnalysis {
        morpheme_ids: morphemes.to_vec(),
        root_morpheme_index: root_index,
        ..crate::test_support::parity_analysis(0)
    }
}

fn proposal(morphemes: &[u32], root_index: i32) -> Candidate {
    Candidate {
        morphemes: morphemes.iter().copied().map(MorphemeId).collect(),
        root_index,
    }
}

#[test]
fn a_proposed_key_for_every_oracle_analysis_is_no_loss_and_the_check_fires() {
    let oracle = [analysis(&[0, 1], 0), analysis(&[2], 0)];
    // Deliberately over-proposing: extra keys are the design (FST proposes, HC prunes), never a defect.
    let proposals = [proposal(&[0, 1], 0), proposal(&[2], 0), proposal(&[9], 0)];
    let mut misses = Vec::new();
    let counters = check_occurrence("w", 0, &oracle, &proposals, &mut misses);
    assert!(misses.is_empty());
    assert_eq!(counters.oracle_keys_required, 2);
    assert_eq!(counters.oracle_keys_matched, 2);
    assert_eq!(counters.oracle_keys_missed, 0);
    assert_eq!(counters.proposal_keys, 3);
    assert_eq!(
        counters.membership_tests, 2,
        "the check must actually execute once per required key"
    );
    assert_eq!(counters.confirmation_calls, 0);
    assert_eq!(counters.confirmation_steps, 0);
    assert_eq!(verdict_from(&counters, misses), AccuracyVerdict::NoLoss);
}

#[test]
fn a_missing_key_is_named_exactly_and_never_rounded_off() {
    let oracle = [analysis(&[0, 1], 0), analysis(&[2], 0)];
    // `[2]` is absent, and `[0, 1]` at root_index 1 is a different key -- headedness is part of the key.
    let proposals = [proposal(&[0, 1], 0), proposal(&[0, 1], 1)];
    let mut misses = Vec::new();
    let counters = check_occurrence("w", 3, &oracle, &proposals, &mut misses);
    assert_eq!(counters.oracle_keys_missed, 1);
    assert_eq!(counters.membership_tests, 2);
    assert_eq!(misses.len(), 1);
    assert_eq!(misses[0].morpheme_ids, vec![2]);
    assert_eq!(misses[0].root_index, 0);
    assert_eq!(misses[0].occurrence_ordinal, 3);
    let verdict = verdict_from(&counters, misses);
    let AccuracyVerdict::Undergenerated { misses } = &verdict else {
        panic!("a lost oracle analysis must read as undergeneration: {verdict:?}");
    };
    assert_eq!(misses.len(), 1);
    assert!(misses[0].to_string().contains("was never proposed"));
}

#[test]
fn root_index_discriminates_headedness_exactly_as_confirm_does() {
    // Same morpheme sequence, different head (`root_index`): both readings are required separately.
    let oracle = [analysis(&[0, 1], 0), analysis(&[0, 1], 1)];
    let mut misses = Vec::new();
    let counters = check_occurrence("w", 0, &oracle, &[proposal(&[0, 1], 0)], &mut misses);
    assert_eq!(counters.oracle_keys_required, 2);
    assert_eq!(counters.oracle_keys_missed, 1);
    assert_eq!(misses[0].root_index, 1);
}

#[test]
fn duplicate_oracle_paths_collapse_and_do_not_demand_duplicate_proposals() {
    // Deduplicated set containment, not multiset: three derivational paths to one analysis still need only one proposed key.
    let oracle = [analysis(&[0], 0), analysis(&[0], 0), analysis(&[0], 0)];
    let mut misses = Vec::new();
    let counters = check_occurrence("w", 0, &oracle, &[proposal(&[0], 0)], &mut misses);
    assert_eq!(counters.oracle_keys_required, 1);
    assert_eq!(counters.membership_tests, 1);
    assert!(misses.is_empty());
}

#[test]
fn an_ambiguous_oracle_key_is_reported_rather_than_hidden() {
    // Two oracle analyses share an admission key but differ in category; key containment is coarser than identity containment.
    let mut second = analysis(&[0], 0);
    second.pos_id = Some(7);
    let mut first = analysis(&[0], 0);
    first.pos_id = Some(1);
    let mut misses = Vec::new();
    let counters = check_occurrence("w", 0, &[first, second], &[proposal(&[0], 0)], &mut misses);
    assert_eq!(counters.oracle_key_ambiguities, 1);
    assert_eq!(counters.oracle_keys_required, 1, "one KEY, two identities");

    // ... and an unambiguous corpus reports zero, so the counter is not a constant.
    let plain = [analysis(&[0], 0), analysis(&[1], 0)];
    let mut misses = Vec::new();
    let counters = check_occurrence("w", 0, &plain, &[], &mut misses);
    assert_eq!(counters.oracle_key_ambiguities, 0);
}

#[test]
fn not_determined_is_never_a_pass() {
    let verdict = AccuracyVerdict::NotDetermined {
        reason: "build failed".into(),
    };
    assert!(
        !verdict.is_no_loss(),
        "an undetermined verdict must not read as no-loss"
    );
    assert!(AccuracyVerdict::NoLoss.is_no_loss());
    assert!(!AccuracyVerdict::Undergenerated { misses: Vec::new() }.is_no_loss());
}

#[test]
fn a_truncated_proposal_set_is_undetermined_rather_than_a_recall_failure() {
    // A refused peel makes the set incomplete -- neither blaming the compilation nor calling it no-loss is honest.
    let counters = AccuracyCounters {
        occurrences_checked: 3,
        oracle_keys_required: 4,
        oracle_keys_missed: 2,
        peel_refusals: 1,
        membership_tests: 4,
        ..AccuracyCounters::default()
    };
    let verdict = verdict_from(&counters, Vec::new());
    assert!(
        matches!(&verdict, AccuracyVerdict::NotDetermined { reason }
            if reason.contains("chain-depth")),
        "a refused peel must not be reported as undergeneration: {verdict:?}"
    );
    // Without the refusal, the SAME miss count is a real recall failure.
    let honest = AccuracyCounters {
        peel_refusals: 0,
        ..counters
    };
    assert!(matches!(
        verdict_from(&honest, Vec::new()),
        AccuracyVerdict::Undergenerated { .. }
    ));
}

#[test]
fn agreeing_about_nothing_is_not_agreement() {
    // If the oracle found no analysis at all, containment holds trivially and an empty network would "pass".
    let vacuous = AccuracyCounters {
        occurrences_checked: 5,
        oracle_keys_required: 0,
        membership_tests: 0,
        ..AccuracyCounters::default()
    };
    assert!(
        matches!(verdict_from(&vacuous, Vec::new()), AccuracyVerdict::NotDetermined { reason }
            if reason.contains("vacuously")),
        "a corpus the oracle analyses not at all must not certify anything"
    );
    // Nor may an empty check.
    assert!(matches!(
        verdict_from(&AccuracyCounters::default(), Vec::new()),
        AccuracyVerdict::NotDetermined { .. }
    ));
    // One real required key is enough to make the verdict meaningful.
    let real = AccuracyCounters {
        occurrences_checked: 1,
        oracle_keys_required: 1,
        oracle_keys_matched: 1,
        membership_tests: 1,
        ..AccuracyCounters::default()
    };
    assert_eq!(verdict_from(&real, Vec::new()), AccuracyVerdict::NoLoss);
}

#[test]
fn the_witness_list_is_capped_while_the_count_stays_exact() {
    let oracle: Vec<WordAnalysis> = (0..MISS_WITNESS_SAMPLE as u32 + 5)
        .map(|n| analysis(&[n], 0))
        .collect();
    let mut misses = Vec::new();
    let counters = check_occurrence("w", 0, &oracle, &[], &mut misses);
    assert_eq!(counters.oracle_keys_missed, MISS_WITNESS_SAMPLE as u64 + 5);
    let AccuracyVerdict::Undergenerated { misses } = verdict_from(&counters, misses) else {
        panic!("every key missing must read as undergeneration");
    };
    assert_eq!(misses.len(), MISS_WITNESS_SAMPLE);
}
