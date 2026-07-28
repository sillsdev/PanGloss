//! **The structural-witness gate** — the last honest prerequisite this crate owns before
//! `tests/conformance_coverage_gate.rs`'s advisory report can responsibly flip to build-breaking.
//!
//! # The gap this file closes
//! Four `machine/conformance/constructs.txt` row ids are each mapped by TWO different
//! [`pg_foma::capability::CharacteristicKind`]s (`pg_foma::conformance_coverage::
//! construct_ids_for`, see that module's own "structural-witness gate" doc section). Three of
//! those pairs are genuinely at risk of one kind's `Covered` status silently INHERITING from a
//! sibling's passing fixture: [`pg_foma::conformance_coverage::supported_coverage_report`] credits
//! a row `Covered` the instant ANY passing word/parse anywhere tags the shared id — it cannot tell
//! "this fixture genuinely exercises the finer construct" from "this fixture exercises the coarser
//! sibling and happens to share the finer one's row id". A hand audit today confirms all three
//! finer characteristics have genuine evidence (see this crate's own task doc / commit history for
//! the citations), but a hand audit is not a gate: if the specific witnessing fixture were later
//! deleted, or stopped passing, while the coarser sibling kept tagging the shared id, the row would
//! keep reporting `Covered` forever. That is precisely the overclaim ADR 0001's cross-check exists
//! to forbid.
//!
//! # What this file does about it
//! For each of [`pg_foma::conformance_coverage::registered_structural_witnesses`]'s three entries,
//! this file replays EVERY discovered fixture (`pg_conformance_fixtures::discover` — the one shared
//! enumeration helper, not a second path walker) against `pg_parse::Morpher` (the same oracle
//! `tests/conformance_coverage_gate.rs` and `pg-parse`'s own `conformance_fixtures_gate.rs` already
//! use) and asserts that **some fixture whose loaded GRAMMAR structurally exhibits the finer
//! construct** (via the witness's own predicate — never a second hand-rolled definition, see each
//! predicate's own doc in `pg_foma::conformance_coverage` for exactly where it reads its facts
//! from) has at least one PASSING word/parse tagging the shared id. A failing word's `exercises:`
//! tags never count, mirroring `passing_covered_constructs`'s own "what 'passing' means here" rule
//! (`pg_foma::conformance_coverage`'s module doc).
//!
//! This cross-checks the grammar SHAPE against the TAG, so the finer characteristic's evidence is
//! mechanically pinned rather than merely hand-reviewed. It does not (and cannot) prove the tag was
//! the RIGHT tag in some deeper semantic sense — same disclaimer `tests/exercises_tag_liveness.rs`
//! already carries for tag-string validity — but it does prove the tag is not resting SOLELY on a
//! coarser sibling's unrelated evidence.
//!
//! # Non-vacuity discipline
//! Every loop below asserts a non-zero scan count independently: zero fixtures discovered, zero
//! fixtures structurally matching a given predicate, and zero registered witnesses are all treated
//! as gate failures in their own right, not silently-passing edge cases — mirroring
//! `tests/coverage_citation_liveness.rs`/`tests/exercises_tag_liveness.rs`'s own "a silently-vacuous
//! check is worse than a loud failure" discipline (read those two files first if this reasoning
//! feels unfamiliar).

use std::collections::HashSet;

use pg_conformance_fixtures::discover;
use pg_foma::conformance_coverage::registered_structural_witnesses;
use pg_grammar::model::Grammar;
use pg_parse::Morpher;

/// Every `constructs.txt` id tagged by a PASSING word/parse belonging to a fixture whose loaded
/// grammar satisfies `predicate`, plus the count of fixtures that structurally matched at all
/// (independent of whether any of their words happened to pass or tag anything) — so a caller can
/// distinguish "no structurally-matching fixture exists" from "one exists but tags nothing".
///
/// Mirrors `tests/conformance_coverage_gate.rs`'s own `passing_covered_constructs` "what counts as
/// passing" rule exactly (replay against the SAME oracle, `pg_parse::Morpher`, crediting only a
/// word/parse whose engine output matches its fixture's declared ground-truth signature) —
/// re-derived independently here rather than imported, matching that file's own "this file
/// re-derives its own passing replay independently... rather than depending on that test's
/// internals" precedent (see its own top-doc).
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

/// **The deliverable itself.** For each of today's four registered structural witnesses: at
/// least one fixture must structurally exhibit the finer construct AND have a passing word/parse
/// tagging the shared id — proving the finer characteristic's evidence is pinned to a real
/// grammar shape, not merely inherited from a coarser sibling's unrelated passing fixture.
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

/// Belt-and-suspenders on [`every_registered_structural_witness_is_satisfied_by_a_passing_fixture`]:
/// names the SPECIFIC fixture this crate's own hand audit identified for each witness (module doc),
/// so a reviewer can see at a glance which fixture is load-bearing for each id, and so a future
/// change that deletes/renames one of these three specific fixtures — while some OTHER fixture
/// happens to still satisfy the general gate above — gets a pointed failure naming exactly which
/// named fixture stopped being discoverable/structurally-matching/passing, rather than only the
/// generic "some fixture somewhere" failure above.
/// One hand-identified witness expectation (a named tuple struct rather than a raw tuple, so the
/// four-field shape stays self-documenting at each call site and clippy's `type_complexity` lint
/// has nothing to flag).
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
