//! Cross-table respelling (`CharacteristicKind::CrossTableRespelling`), ratcheted in both directions: which fixtures the characterizer flags, and what every backend does with each. A fixture gaining or losing the construct, or a backend flipping between exact and refused, fails here by name.
//! See docs/research/grill-me-queue.md (G11) for how the construct was misattributed to rewrite-rule feature erasure and how hc.dll settled it.

use std::collections::BTreeMap;

use pg_conformance_fixtures::discover;
use pg_foma::capability::{characterize, CharacteristicKind};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::scoreboard::{self, CellOutcome};
use pg_grammar::model::Grammar;

/// What one backend does with a respelling fixture. There is deliberately no third value: a
/// backend that compiles and misses the respelled analysis is the defect this file exists to catch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expected {
    Exact,
    Refused,
}

/// Every fixture the characterizer flags today, with each backend's verdict in `ALL_STRATEGIES`
/// order (PlanComposed, TunedSurfaceProbed, TemplatedUnderlyingTokens).
const EXPECTED: &[(&str, [Expected; 3])] = &[
    // The isolating fixture: no phonological rule anywhere, disjoint alphabets, respelling alone. PlanComposed is exact here and stays exact under `plan_composed_spells_by_bundle_not_by_index`; on the three fixtures below it refuses for an unrelated reason (a composite-marker plan subtree it does not build).
    (
        "machine:edge-cases/cross-table-root-respelling",
        [Expected::Exact, Expected::Exact, Expected::Refused],
    ),
    (
        "machine:edge-cases/rewrite-analysis-feature-neutralization",
        [Expected::Refused, Expected::Exact, Expected::Refused],
    ),
    // The respelled root ("w" as the final table's "i") is then changed by a rule; the construct is present even though hc.dll renders the signature's surface half empty.
    (
        "machine:edge-cases/synthesis-stratum-render-stale-table",
        [Expected::Refused, Expected::Exact, Expected::Refused],
    ),
    (
        "staging:edge-cases/segment-natural-class-table-binding",
        [Expected::Refused, Expected::Exact, Expected::Refused],
    ),
];

fn respelling_fixtures() -> BTreeMap<String, (Grammar, Vec<String>)> {
    let mut found = BTreeMap::new();
    for fixture in discover() {
        let words_yaml = fixture.load_words_yaml();
        if words_yaml.expect_crash {
            continue;
        }
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        let profile = characterize(&grammar);
        let flagged = profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::CrossTableRespelling);
        if !flagged {
            continue;
        }
        let details: Vec<String> = profile
            .cross_table_respelling_details()
            .map(|d| format!("{:?} {:?}->{:?}", d.allomorph, d.own_spelling, d.surface_spelling))
            .collect();
        eprintln!("{}: cross-table respelling {}", fixture.label(), details.join(", "));
        let words = words_yaml.words.iter().map(|w| w.word.clone()).collect();
        found.insert(fixture.label(), (grammar, words));
    }
    found
}

/// Both directions on the fixture set: a new fixture exhibiting the construct must be added to
/// `EXPECTED`, and a fixture that stops exhibiting it (a grammar edit, or a characterizer that
/// went quiet) is reported rather than silently dropping out of the backend check below.
#[test]
fn cross_table_respelling_is_observed_on_exactly_the_known_fixtures() {
    let observed: Vec<String> = respelling_fixtures().keys().cloned().collect();
    let expected: Vec<String> = EXPECTED.iter().map(|(label, _)| label.to_string()).collect();
    assert_eq!(
        observed, expected,
        "the set of fixtures whose characterizer profile carries CrossTableRespelling changed; \
         update EXPECTED with each backend's measured verdict"
    );
}

/// Soundness first (no candidate-only survivors anywhere), then the per-backend ratchet: the
/// mainline stays oracle-exact, and the two token-route backends refuse typed rather than miss.
#[test]
fn every_respelling_fixture_is_oracle_exact_on_tsp_and_typed_elsewhere() {
    let fixtures = respelling_fixtures();
    assert!(!fixtures.is_empty(), "no fixture exhibits the construct; the gate would be vacuous");
    // Every cell is measured and printed before anything is asserted, so one flipped verdict never hides the others.
    let mut mismatches = Vec::new();
    for (label, expected) in EXPECTED {
        let Some((grammar, words)) = fixtures.get(*label) else {
            panic!("{label}: listed in EXPECTED but the characterizer no longer flags it");
        };
        let scored = scoreboard::measure(label, grammar, words);
        for (strategy, want) in pg_foma::strategy_coverage::ALL_STRATEGIES
            .iter()
            .zip(expected.iter())
        {
            let cell = scored
                .cells
                .iter()
                .find(|c| c.strategy == *strategy)
                .unwrap_or_else(|| panic!("{label}: {strategy:?} has no cell"));
            eprintln!(
                "{label} [{strategy:?}]: {:?} (certification={})",
                cell.outcome, cell.certification_debug
            );
            if let Some(div) = cell.divergence {
                assert_eq!(
                    div.candidate_only_identities, 0,
                    "{label} [{strategy:?}]: candidate-only identities must stay 0"
                );
            }
            let got = match &cell.outcome {
                CellOutcome::OracleExact => Expected::Exact,
                CellOutcome::Refused { .. } => Expected::Refused,
                other => panic!(
                    "{label} [{strategy:?}]: {other:?} -- a backend must either propose the \
                     respelled analysis or refuse the grammar typed; certification={}",
                    cell.certification_debug
                ),
            };
            if got != *want {
                mismatches.push(format!(
                    "{label} [{strategy:?}]: expected {want:?}, got {got:?} (certification={})",
                    cell.certification_debug
                ));
            }
            if *strategy == EmissionStrategy::TunedSurfaceProbed {
                let div = cell.divergence.expect("OracleExact carries a divergence delta");
                assert_eq!(div.oracle_only_identities, 0, "{label}: exact yet misses identities");
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "backend verdicts moved; if a backend now represents the construct, move its \
         strategy_coverage row and this ratchet together:\n  {}",
        mismatches.join("\n  ")
    );
}

/// The discriminating experiment behind PlanComposed's `Represents` row: the isolating fixture
/// declares the inner root "m" at index 0 and the same-bundle final segment "t" at index 0, so an
/// exact result there could be raw-index coincidence. Listing the final table's other segment
/// first moves "t" to index 1 without changing a single bundle; a backend that spells by bundle is
/// unmoved, one that spells by index is not. First run: unmoved.
#[test]
fn plan_composed_spells_by_bundle_not_by_index() {
    let fixture = discover()
        .into_iter()
        .find(|f| f.label() == "machine:edge-cases/cross-table-root-respelling")
        .expect("the isolating fixture is discovered under PANGLOSS_CONFORMANCE_SCOPE=all");
    let xml = fixture.load_grammar_xml();
    let block = |id: &str| -> String {
        let start = xml
            .find(&format!("<SegmentDefinition id=\"{id}\">"))
            .unwrap_or_else(|| panic!("{id} block missing"));
        let end = xml[start..]
            .find("</SegmentDefinition>")
            .map(|i| start + i + "</SegmentDefinition>".len())
            .unwrap_or_else(|| panic!("{id} block unterminated"));
        xml[start..end].to_string()
    };
    let (cb1, cb2) = (block("cB1"), block("cB2"));
    let reordered = xml.replacen(&cb1, "\u{0}", 1).replacen(&cb2, &cb1, 1).replacen("\u{0}", &cb2, 1);
    assert_ne!(reordered, xml, "the reordering must change the document");
    let grammar = pg_grammar::load(&reordered).expect("reordered grammar loads");
    let words = fixture
        .load_words_yaml()
        .words
        .iter()
        .map(|w| w.word.clone())
        .collect::<Vec<_>>();
    let scored = scoreboard::measure("reordered isolating fixture", &grammar, &words);
    let outcome = |strategy: EmissionStrategy| {
        &scored
            .cells
            .iter()
            .find(|c| c.strategy == strategy)
            .unwrap_or_else(|| panic!("{strategy:?} has no cell"))
            .outcome
    };
    for cell in &scored.cells {
        eprintln!("reordered [{:?}]: {:?}", cell.strategy, cell.outcome);
    }
    assert!(
        matches!(outcome(EmissionStrategy::TunedSurfaceProbed), CellOutcome::OracleExact),
        "the mainline spells by bundle and must not care about table order"
    );
    assert!(
        matches!(outcome(EmissionStrategy::PlanComposed), CellOutcome::OracleExact),
        "PlanComposed lost exactness when the final table was reordered: its token resolution has \
         become index-dependent, so its strategy_coverage row for CrossTableRespelling can no \
         longer say Represents"
    );
}
