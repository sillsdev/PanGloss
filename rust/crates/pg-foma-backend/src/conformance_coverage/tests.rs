use super::*;

/// Every `CharacteristicKind` must be reachable from `construct_ids_for` without panicking end to end for the full `ALL` list.
#[test]
fn construct_ids_for_is_callable_for_every_kind() {
    for &kind in CharacteristicKind::ALL {
        let _ = construct_ids_for(kind);
    }
}

/// `supported_kinds` must be exactly `CharacteristicKind::ALL`; narrowing to the `Proven` subset would make the cross-check vacuous for most of the ledger.
#[test]
fn supported_kinds_is_exactly_characteristic_kind_all() {
    let supported = supported_kinds();
    assert_eq!(supported, CharacteristicKind::ALL.to_vec());
    // Sanity: strictly wider than the old Proven-only filter, guarding against silently narrowing this back down.
    let proven_only = CharacteristicKind::ALL
        .iter()
        .copied()
        .filter(|k| k.default_disposition() == Disposition::Proven)
        .count();
    assert!(supported.len() > proven_only);
}

/// With an empty evidence set, every kind must be `Uncovered` or `Unmappable`, never `Covered`.
#[test]
fn empty_evidence_sets_yield_no_covered_rows() {
    let covered: HashSet<&str> = HashSet::new();
    let report = supported_coverage_report(&covered);
    assert!(!report.is_empty());
    for row in &report {
        assert_ne!(row.status, CoverageStatus::Covered, "{:?}", row.kind);
    }
}

/// After G9, no `CharacteristicKind` is `Unmappable`; unlike `Covered`, this depends only on `construct_ids_for` being non-empty, not on caller-supplied evidence.
#[test]
fn zero_unmappable_after_g9() {
    let covered: HashSet<&str> = HashSet::new();
    let report = supported_coverage_report(&covered);
    let unmappable: Vec<CharacteristicKind> = report
        .iter()
        .filter(|r| r.status == CoverageStatus::Unmappable)
        .map(|r| r.kind)
        .collect();
    assert!(
        unmappable.is_empty(),
        "G9 must leave zero Unmappable kinds; found {unmappable:?}"
    );
    for &kind in CharacteristicKind::ALL {
        assert!(
            !construct_ids_for(kind).is_empty(),
            "{kind:?} still has no constructs.txt mapping after G9"
        );
    }
}

/// Feeding in every construct id `construct_ids_for` ever names must make every kind `Covered`.
#[test]
fn fully_evidenced_sets_cover_every_kind() {
    let mut covered: HashSet<&str> = HashSet::new();
    for &kind in CharacteristicKind::ALL {
        for &id in construct_ids_for(kind) {
            covered.insert(id);
        }
    }
    let report = supported_coverage_report(&covered);
    for row in &report {
        assert_eq!(
            row.status,
            CoverageStatus::Covered,
            "{:?} has mapped construct ids {:?} but was not Covered",
            row.kind,
            row.construct_ids
        );
    }
}

/// `MprGroupOverwrite` shares `"MPR features/groups"` with `MprGroupAppend`, the shared-id inheritance `registered_structural_witnesses` pins structurally rather than by disposition.
#[test]
fn overwrite_row_uses_passing_fixture_evidence() {
    let mut covered: HashSet<&str> = HashSet::new();
    for &id in construct_ids_for(CharacteristicKind::MprGroupAppend) {
        covered.insert(id);
    }
    let report = supported_coverage_report(&covered);
    let overwrite_row = report
        .iter()
        .find(|r| r.kind == CharacteristicKind::MprGroupOverwrite)
        .expect("MprGroupOverwrite must appear in the ledger-wide report");
    assert_eq!(overwrite_row.status, CoverageStatus::Covered);
}

/// `supported_uncovered` is exactly the non-`Covered` projection of the same report.
#[test]
fn supported_uncovered_matches_report_non_covered_rows() {
    let covered: HashSet<&str> = HashSet::new();
    let report = supported_coverage_report(&covered);
    let uncovered = supported_uncovered(&covered);
    let expected: Vec<CharacteristicKind> = report
        .iter()
        .filter(|r| r.status != CoverageStatus::Covered)
        .map(|r| r.kind)
        .collect();
    assert_eq!(uncovered, expected);
}

// Structural-witness gate, generic non-fixture half; the fixture-scanning half lives in tests/structural_witness_gate.rs.

/// `shared_construct_ids` must find the known four shared ids today, pinning the exact shape so a silent change to `construct_ids_for` shows up here too.
#[test]
fn shared_construct_ids_finds_the_four_known_pairs() {
    let shared = shared_construct_ids();
    let pairs_containing = |a: CharacteristicKind, b: CharacteristicKind| {
        shared
            .iter()
            .any(|(_, kinds)| kinds.contains(&a) && kinds.contains(&b))
    };
    assert!(
        pairs_containing(
            CharacteristicKind::OrderedMorphRuleApplication,
            CharacteristicKind::UnorderedMorphRuleApplication
        ),
        "{shared:?}"
    );
    assert!(
        pairs_containing(
            CharacteristicKind::IterativeRewrite,
            CharacteristicKind::Epenthesis
        ),
        "{shared:?}"
    );
    assert!(
        pairs_containing(
            CharacteristicKind::Affixation,
            CharacteristicKind::CircumfixOutputAction
        ),
        "{shared:?}"
    );
    assert!(
        pairs_containing(
            CharacteristicKind::MprGroupAppend,
            CharacteristicKind::MprGroupOverwrite
        ),
        "{shared:?}"
    );
    assert_eq!(shared.len(), 4, "{shared:?}");
}

/// The generic drift guard: every shared id must have a registered `StructuralWitness`, computed with no hardcoded id list, so a new shared-id pair with no structural predicate fails loudly.
#[test]
fn every_shared_id_has_a_registered_structural_witness() {
    let at_risk = shared_construct_ids();
    assert!(
        !at_risk.is_empty(),
        "the at-risk-shared-id scan found nothing -- construct_ids_for's shape changed and \
         this check went vacuous, which is worse than a failure (see this module's own \
         top-doc on silent rot)"
    );

    let registered: HashSet<&str> = registered_structural_witnesses()
        .iter()
        .map(|w| w.construct_id)
        .collect();
    assert!(
        !registered.is_empty(),
        "no structural witnesses registered at all"
    );

    let missing: Vec<String> = at_risk
        .iter()
        .filter(|(id, _)| !registered.contains(id))
        .map(|(id, kinds)| {
            format!("{id:?} (shared by {kinds:?}) has no registered StructuralWitness")
        })
        .collect();
    assert!(
        missing.is_empty(),
        "new at-risk shared construct id(s) with no structural witness -- add one to \
         registered_structural_witnesses before trusting these ids' Covered status:\n  {}",
        missing.join("\n  ")
    );
}

/// Each witness's `construct_id` must be one of `construct_ids_for`'s ids for its own `finer_kind`, catching a witness pointing at the wrong kind's id.
#[test]
fn each_registered_witness_construct_id_matches_its_finer_kind() {
    for w in registered_structural_witnesses() {
        assert!(
            construct_ids_for(w.finer_kind).contains(&w.construct_id),
            "{w:?}: construct_id is not among construct_ids_for(finer_kind)"
        );
    }
}
