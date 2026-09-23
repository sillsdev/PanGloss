use super::*;
use crate::capability::{default_registry, undischarged_kinds};

/// A fixed, hand-built passing set so these ledgers are deterministic and independent of any real fixture's pass/fail state.
fn fully_covered_constructs() -> HashSet<&'static str> {
    let mut set = HashSet::new();
    for &kind in CharacteristicKind::ALL {
        for &id in construct_ids_for(kind) {
            set.insert(id);
        }
    }
    set
}

// Exhaustiveness / no-drift

/// Every `CharacteristicKind` appears in the built ledger exactly once.
#[test]
fn every_characteristic_kind_appears_exactly_once() {
    let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
    assert_eq!(ledger.rows.len(), CharacteristicKind::ALL.len());
    for &kind in CharacteristicKind::ALL {
        let count = ledger.rows.iter().filter(|r| r.kind == kind).count();
        assert_eq!(count, 1, "{kind:?} must appear exactly once in the ledger");
    }
}

/// Every row's `disposition` is always exactly `kind.default_disposition()`, never a hardcoded copy.
#[test]
fn ledger_disposition_never_diverges_from_default_disposition() {
    let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
    for row in &ledger.rows {
        assert_eq!(
            row.disposition,
            row.kind.default_disposition(),
            "{:?}'s ledger disposition diverged from default_disposition()",
            row.kind
        );
    }
}

/// Every `ConfigPredicate` row names at least one discharging predicate, agreeing with `undischarged_kinds`.
#[test]
fn every_config_predicate_row_names_a_discharging_predicate() {
    let registry = default_registry();
    assert!(
        undischarged_kinds(&registry).is_empty(),
        "sanity: default_registry() is expected to already discharge every ConfigPredicate kind"
    );
    let ledger = build_ledger(&registry, &HashSet::new());
    for row in &ledger.rows {
        if row.disposition == Disposition::ConfigPredicate {
            assert!(
                !row.discharging_predicates.is_empty(),
                "{:?} is {:?} but the ledger names no discharging predicate",
                row.kind,
                row.disposition
            );
        }
    }
}

/// `containment_evidence_for` must be callable end to end for every kind.
#[test]
fn containment_evidence_for_is_callable_for_every_kind() {
    for &kind in CharacteristicKind::ALL {
        let _ = containment_evidence_for(kind);
    }
}

// Evidence names its strategies (the coverage-gate inheritance trap, per-strategy axis)

/// A construct claimed covered must name the strategies it was demonstrated on; no citation may be unattributed or name a compiler that does not exist.
#[test]
fn every_containment_citation_names_at_least_one_real_strategy() {
    let valid: HashSet<&str> = crate::strategy_coverage::ALL_STRATEGIES
        .iter()
        .map(|s| s.label())
        .collect();
    for &kind in CharacteristicKind::ALL {
        let Some(evidence) = containment_evidence_for(kind) else {
            continue;
        };
        assert!(
            !evidence.strategies.is_empty(),
            "{kind:?}'s containment citation names no strategy: {}",
            evidence.citation
        );
        for named in &evidence.strategies {
            assert!(
                valid.contains(named.as_str()),
                "{kind:?}'s citation names an unknown strategy {named:?}"
            );
        }
    }
}

/// A citation may never name a strategy the per-strategy account says cannot represent the construct -- either the citation or the table row is wrong.
#[test]
fn no_citation_claims_a_strategy_that_cannot_represent_the_construct() {
    for &kind in CharacteristicKind::ALL {
        let Some(evidence) = containment_evidence_for(kind) else {
            continue;
        };
        for &strategy in crate::strategy_coverage::ALL_STRATEGIES {
            if !evidence.strategies.iter().any(|s| s == strategy.label()) {
                continue;
            }
            assert_ne!(
                representation_of(strategy, kind).representation,
                crate::strategy_coverage::StrategyRepresentation::CannotRepresent,
                "{kind:?}'s citation claims {strategy:?}, which the strategy account says \
                 cannot represent the construct at all"
            );
        }
    }
}

/// `strategies_unwitnessed` must be derived (representing set minus evidence set), not hand-maintained, and genuinely non-empty somewhere.
#[test]
fn unwitnessed_strategies_are_derived_and_the_gap_is_reported_not_hidden() {
    let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
    let mut any_gap = false;
    for row in &ledger.rows {
        let witnessed: Vec<String> = row
            .containment
            .as_ref()
            .map(|e| e.strategies.clone())
            .unwrap_or_default();
        let expected: Vec<String> = strategies_that_represent(row.kind)
            .into_iter()
            .map(|s| s.label().to_string())
            .filter(|l| !witnessed.contains(l))
            .collect();
        assert_eq!(
            row.strategies_unwitnessed, expected,
            "{:?}'s unwitnessed set must be derived, never hand-written",
            row.kind
        );
        any_gap |= !row.strategies_unwitnessed.is_empty();
    }
    assert!(
        any_gap,
        "no row reports an unwitnessed strategy -- either every construct now has a witness on \
         every compiler that can represent it (check before believing it), or the derivation \
         silently collapsed"
    );
}

/// The per-row `CannotRepresent` list makes the live `PlanComposed`/`TemplatedUnderlyingTokens` x `ProcessMorphology` hole visible in the ledger, not only in the selection path.
#[test]
fn the_ledger_reports_the_live_whole_construct_hole() {
    let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
    let row = ledger
        .row(CharacteristicKind::ProcessMorphology)
        .expect("row must exist");
    assert_eq!(
        row.strategies_cannot_represent,
        vec![
            EmissionStrategy::PlanComposed.label().to_string(),
            EmissionStrategy::TemplatedUnderlyingTokens
                .label()
                .to_string(),
        ]
    );
    assert!(
        ledger
            .row(CharacteristicKind::Affixation)
            .expect("row must exist")
            .strategies_cannot_represent
            .is_empty(),
        "no strategy fails to represent ordinary affixation"
    );
}

/// `NaturalClassDefinition` and `FreeFluctuation` are the deliberate, documented `None`s; a future edit that starts or stops returning evidence for either must be a reviewed, visible change.
#[test]
fn every_kind_without_a_containment_witness_is_named_and_justified() {
    let missing: Vec<CharacteristicKind> = CharacteristicKind::ALL
        .iter()
        .copied()
        .filter(|&k| containment_evidence_for(k).is_none())
        .collect();
    assert_eq!(
        missing,
        vec![
            CharacteristicKind::NaturalClassDefinition,
            CharacteristicKind::FreeFluctuation,
            // No test drives an ablaut grammar through propose-then-confirm on ANY backend.
            CharacteristicKind::ProcessMorphology
        ],
        "every kind without a containment witness must be named here with a reason -- an \
         unexplained addition means somebody added a construct and skipped its witness"
    );
}

// build_ledger: conformance_status classification

#[test]
fn build_ledger_with_empty_passing_set_never_marks_a_fixture_evidenced_row_covered() {
    let ledger = build_ledger(&default_registry(), &HashSet::new());
    for row in &ledger.rows {
        assert_ne!(
            row.conformance_status,
            CoverageStatus::Covered,
            "{:?}",
            row.kind
        );
    }
}

#[test]
fn build_ledger_with_fully_covered_set_covers_every_mappable_row() {
    let covered = fully_covered_constructs();
    let ledger = build_ledger(&default_registry(), &covered);
    for row in &ledger.rows {
        if row.construct_ids.is_empty() {
            assert_eq!(
                row.conformance_status,
                CoverageStatus::Unmappable,
                "{:?}",
                row.kind
            );
        } else {
            assert_eq!(
                row.conformance_status,
                CoverageStatus::Covered,
                "{:?}",
                row.kind
            );
        }
    }
}

/// Zero ledger rows are `Unmappable`, unconditionally -- depends only on `construct_ids_for` being non-empty per kind, never on the passing-fixture set.
#[test]
fn zero_unmappable_rows_after_g9() {
    let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
    let unmappable: Vec<CharacteristicKind> = ledger
        .rows
        .iter()
        .filter(|r| r.conformance_status == CoverageStatus::Unmappable)
        .map(|r| r.kind)
        .collect();
    assert!(
        unmappable.is_empty(),
        "expected zero Unmappable ledger rows; found {unmappable:?}"
    );
    for row in &ledger.rows {
        assert!(
            !row.construct_ids.is_empty(),
            "{:?} still has no constructs.txt mapping after G9",
            row.kind
        );
    }
}

#[test]
fn row_accessor_finds_every_kind_exactly_once() {
    let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
    for &kind in CharacteristicKind::ALL {
        assert!(
            ledger.row(kind).is_some(),
            "{kind:?} must be findable via row()"
        );
    }
}

// Canonical JSON: golden + round trip

/// A deterministic, fully-covered-set ledger, so this golden stays stable regardless of unrelated fixture churn.
fn golden_ledger() -> CoverageLedger {
    build_ledger(&default_registry(), &fully_covered_constructs())
}

#[track_caller]
fn assert_coverage_ledger_golden(actual: &str, expected: &str) {
    crate::test_support::assert_canonical_lf_text_eq(actual, expected);
}

#[test]
fn coverage_ledger_golden_boundary_accepts_lf_actual_against_crlf_expected() {
    let actual = "{\n  \"schema_version\": 1\n}\n";
    let expected = actual.replace('\n', "\r\n");
    assert_ne!(actual, expected);
    assert_coverage_ledger_golden(actual, &expected);
}

#[test]
fn coverage_ledger_golden_boundary_rejects_crlf_actual() {
    let actual = "{\n  \"schema_version\": 1\n}\n";
    let expected = "{\n  \"schema_version\": 1\n}\n";
    let crlf_actual = actual.replace('\n', "\r\n");
    assert_ne!(crlf_actual, expected);
    let panic = std::panic::catch_unwind(|| {
        assert_coverage_ledger_golden(&crlf_actual, expected);
    });
    assert!(panic.is_err());
}

#[test]
fn coverage_ledger_golden_boundary_rejects_ordering_and_trailing_newline_drift() {
    let ordering = std::panic::catch_unwind(|| {
        assert_coverage_ledger_golden(
            "{\n  \"a\": 1,\n  \"b\": 2\n}\n",
            "{\n  \"b\": 2,\n  \"a\": 1\n}\n",
        );
    });
    assert!(ordering.is_err());

    let trailing_newline = std::panic::catch_unwind(|| {
        assert_coverage_ledger_golden(
            "{\n  \"schema_version\": 1\n}",
            "{\n  \"schema_version\": 1\n}\n",
        );
    });
    assert!(trailing_newline.is_err());
}

#[test]
fn coverage_ledger_round_trip() {
    let ledger = golden_ledger();
    let json = ledger.to_json().expect("serialization must succeed");
    let parsed = CoverageLedger::from_json(&json).expect("deserialization must succeed");
    assert_eq!(
        parsed, ledger,
        "round trip through canonical JSON must be lossless"
    );
}

#[test]
fn coverage_ledger_schema_version_is_stamped() {
    assert_eq!(
        golden_ledger().schema_version,
        COVERAGE_LEDGER_SCHEMA_VERSION
    );
}

#[test]
#[ignore = "regeneration helper, not a gate: run with --ignored to rewrite the golden from this \
            test's own computation after a reviewed citation/predicate change"]
fn regenerate_coverage_ledger_golden_json() {
    let json = golden_ledger()
        .to_json()
        .expect("serialization must succeed");
    std::fs::write(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/coverage_ledger_golden.json"
        ),
        json,
    )
    .expect("golden must be writable");
}

#[test]
fn coverage_ledger_golden_json() {
    let ledger = golden_ledger();
    let json = ledger.to_json().expect("serialization must succeed");
    assert_coverage_ledger_golden(&json, GOLDEN_JSON);
}

const GOLDEN_JSON: &str = include_str!("../coverage_ledger_golden.json");
