//! Closes a coverage-inheritance trap: several `constructs.txt` row ids are shared by two `CharacteristicKind`s, and `supported_coverage_report` credits a row `Covered` the instant any passing fixture anywhere tags the shared id — it cannot tell a finer construct's own evidence from a coarser sibling's. This file cross-checks the grammar SHAPE against the TAG: for each registered structural witness, at least one fixture whose loaded grammar structurally exhibits the finer construct (via the witness's own predicate) must also have a passing word/parse tagging the shared id, so the finer characteristic's `Covered` status can never rest solely on a sibling's unrelated evidence. Every loop treats a zero scan count as a gate failure in its own right, never a silently-passing edge case.

use std::collections::HashSet;

use pg_conformance_fixtures::discover;
use pg_foma::conformance_coverage::registered_structural_witnesses;
use pg_grammar::model::Grammar;
use pg_parse::Morpher;

/// Every `constructs.txt` id tagged by a passing word/parse in a fixture whose grammar satisfies `predicate`, plus how many fixtures structurally matched at all — so a caller can tell "none exist" from "one exists but tags nothing". Replay rules mirror (independently re-derived, not imported from) `tests/conformance_coverage_gate.rs`'s `passing_covered_constructs`.
fn passing_ids_from_structurally_matching_fixtures(
    predicate: fn(&Grammar) -> bool,
) -> (HashSet<String>, usize) {
    let mut ids: HashSet<String> = HashSet::new();
    let mut structurally_matching_fixtures = 0usize;

    for f in discover() {
        let words_yaml = f.load_words_yaml();
        if words_yaml.skip_in_generic_replay().is_some() {
            continue; // expect_crash / budget_ms: no signature ground truth to replay
        }

        let xml = f.load_grammar_xml();
        let Ok(grammar) = pg_grammar::load(&xml) else {
            continue; // a fixture this gate can't even load contributes no evidence either way
        };
        if !predicate(&grammar) {
            continue;
        }
        structurally_matching_fixtures += 1;

        let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);
        for w in &words_yaml.words {
            if !w.adapter_visible() {
                continue; // self-check-only (guess:true parse), PROTOCOL.md section 3
            }
            if w.expect_skip {
                continue; // SKIPPED words carry no meaningful "matched ground truth" signal
            }
            let outcome = morpher.parse_word(&w.word);
            if outcome.invalid_shape {
                continue; // unexpectedly SKIPPED -> not passing
            }
            if outcome.signature() != w.expected_signature() {
                continue; // mismatch -> not passing; this word's exercises: tags don't count
            }
            for c in &w.exercises {
                ids.insert(c.clone());
            }
            for p in &w.parses {
                for c in &p.exercises {
                    ids.insert(c.clone());
                }
            }
        }
    }

    (ids, structurally_matching_fixtures)
}

/// For each registered structural witness: at least one fixture must structurally exhibit the finer construct AND have a passing word/parse tagging the shared id, proving the evidence is pinned to a real grammar shape, not inherited from a coarser sibling.
#[test]
fn every_registered_structural_witness_is_satisfied_by_a_passing_fixture() {
    let witnesses = registered_structural_witnesses();
    assert_eq!(
        witnesses.len(),
        4,
        "expected exactly today's four live at-risk-shared-id witnesses: {witnesses:?}"
    );

    let mut total_structurally_matching = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for w in &witnesses {
        let (ids, matching) = passing_ids_from_structurally_matching_fixtures(w.predicate);
        total_structurally_matching += matching;

        if matching == 0 {
            failures.push(format!(
                "{:?} (shared id {:?}): zero discovered fixtures structurally exhibit this \
                 construct at all -- the witness predicate itself found nothing to check",
                w.finer_kind, w.construct_id
            ));
            continue;
        }

        if !ids.contains(w.construct_id) {
            failures.push(format!(
                "{:?} (shared id {:?}): {matching} fixture(s) structurally exhibit the \
                 construct, but none of their PASSING words/parses tag the shared id -- its \
                 Covered status rests entirely on a coarser sibling's evidence, exactly the \
                 silent-inheritance risk this gate exists to close",
                w.finer_kind, w.construct_id
            ));
        }
    }

    assert!(
        total_structurally_matching > 0,
        "scanned zero structurally-matching fixtures across ALL THREE witnesses combined -- this \
         gate went vacuous, which is worse than a failure"
    );
    assert!(
        failures.is_empty(),
        "structural-witness gate failure(s):\n  {}",
        failures.join("\n  ")
    );
}

/// Belt-and-suspenders on the general gate above: names the specific load-bearing fixture per witness, so deleting/renaming one gets a pointed failure even if some other fixture still satisfies the general check.
struct WitnessExpectation {
    fixture_name: &'static str,
    construct_id: &'static str,
    predicate: fn(&Grammar) -> bool,
    shape_label: &'static str,
}

#[test]
fn the_hand_identified_witness_fixture_for_each_construct_still_qualifies() {
    let expectations = [
        WitnessExpectation {
            fixture_name: "polysynthetic-stratal-derivation-chain",
            construct_id: "Stratum (Linear/Unordered rule order)",
            predicate: pg_foma::conformance_coverage::grammar_has_unordered_stratum,
            shape_label: "unordered stratum",
        },
        WitnessExpectation {
            fixture_name: "suffixing-vowel-harmony",
            construct_id: "RewriteRule Iterative (epenthesis/deletion/feature/expansion/merge)",
            predicate: pg_foma::conformance_coverage::grammar_has_empty_lhs_rewrite_rule,
            shape_label: "empty-PhoneticInput (epenthesis) rewrite rule",
        },
        WitnessExpectation {
            fixture_name: "fusional-realizational-morphology",
            construct_id: "AffixProcessRule: prefix/suffix/circumfix/infix",
            predicate: pg_foma::conformance_coverage::grammar_has_circumfix_shaped_allomorph,
            shape_label: "circumfix-shaped allomorph",
        },
    ];

    let fixtures = discover();
    assert!(!fixtures.is_empty(), "discover() returned zero fixtures");

    let mut checked = 0usize;
    for WitnessExpectation {
        fixture_name: name,
        construct_id,
        predicate,
        shape_label,
    } in &expectations
    {
        let f = fixtures
            .iter()
            .find(|f| f.name == *name)
            .unwrap_or_else(|| panic!("named witness fixture {name:?} was not discovered at all"));

        let xml = f.load_grammar_xml();
        let grammar = pg_grammar::load(&xml)
            .unwrap_or_else(|e| panic!("{}: failed to load grammar.xml: {e}", f.label()));
        assert!(
            predicate(&grammar),
            "{}: expected this fixture's grammar to structurally exhibit a {shape_label}",
            f.label()
        );

        let words_yaml = f.load_words_yaml();
        assert!(
            words_yaml.skip_in_generic_replay().is_none(),
            "{}: this named witness fixture must be generic-replay-eligible",
            f.label()
        );
        let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);
        let mut tags_it_while_passing = false;
        for w in &words_yaml.words {
            if !w.adapter_visible() || w.expect_skip {
                continue;
            }
            let outcome = morpher.parse_word(&w.word);
            if outcome.invalid_shape || outcome.signature() != w.expected_signature() {
                continue;
            }
            if w.exercises.iter().any(|c| c == construct_id)
                || w.parses
                    .iter()
                    .any(|p| p.exercises.iter().any(|c| c == construct_id))
            {
                tags_it_while_passing = true;
                break;
            }
        }
        assert!(
            tags_it_while_passing,
            "{}: expected a PASSING word/parse tagging {construct_id:?}",
            f.label()
        );
        checked += 1;
    }

    assert_eq!(
        checked,
        expectations.len(),
        "not every named witness fixture was checked"
    );
}
