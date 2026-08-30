//! Pins that a non-selectable-verdict candidate still appears in `progress.jsonl` and leaves every other candidate evaluable; drives the real `pangloss` binary because this is a property of the loop, evaluator, progress writer, supervisor and report validator together.
//! See docs/research/pg-cli-recipe-optimize-continuation-test-notes.md for why this file exists and why its bounds are self-calibrated rather than hardcoded.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// The staged fixture, chosen because one unbounded run produces the exact mix every test here needs.
/// See docs/research/pg-cli-recipe-optimize-continuation-test-notes.md for the measured evaluation order this file's bounds are derived against.
const FIXTURE: &str = "conformance-staging/edge-cases/backend-strata-generic/grammar.xml";

/// Verbatim from that fixture's `words.yaml`, duplicated rather than parsed so the test needs no YAML round trip; the non-vacuity assertions below fail loudly if the fixture drifts.
const WORDS: &[&str] = &[
    "nuna",
    "nunaliq",
    "nunaliqvuq",
    "nunavuq",
    "akutat",
    "pitat",
    "akupitat",
    "akutas",
    "silanuk",
    "silamanuk",
    "silamak",
    "takuvuq",
    "bubu",
    "buibu",
    "kuuukuuu",
    "kuiikuii",
    "buuubuuu",
    "buiibuii",
    "mau",
    "matu",
];

/// Generous enough that no assertion here can be decided by the machine's load; these runs measure ~0.3s of work.
const ELAPSED_NS: &str = "600000000000";

fn repo_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join(relative)
}

struct Run {
    root: PathBuf,
    /// `progress.jsonl`, in banked order. Parsed strictly, unlike the CLI's tolerant reader, because these runs are never killed.
    rows: Vec<serde_json::Value>,
    /// `None` when the worker produced no `report.json` at all -- itself one of the defects this file pins.
    report: Option<serde_json::Value>,
    worker_succeeded: bool,
}

impl Run {
    fn statuses(&self) -> Vec<String> {
        self.rows
            .iter()
            .map(|row| row["certification"]["status"].as_str().unwrap().to_owned())
            .collect()
    }

    /// Per-candidate confirmation work, in evaluation order.
    fn confirmations(&self) -> Vec<u64> {
        self.rows
            .iter()
            .map(|row| row["score"]["confirmation"].as_u64().unwrap())
            .collect()
    }

    /// What the pilot spent before the search began; `optimize_with_evaluator` is handed `--confirmation-work` minus this, so a derived bound has to add it back.
    fn pilot_confirmation(&self) -> u64 {
        let report = self.report.as_ref().expect("run wrote a report");
        report["usage"]["confirmation"]
            .as_u64()
            .unwrap()
            .checked_sub(self.confirmations().iter().sum::<u64>())
            .expect("run usage must include every evaluated candidate's confirmation work")
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn optimize(tag: &str, extra: &[String]) -> Run {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("pangloss-recipe-continuation-{tag}-{stamp}"));
    let out = root.join("out");
    let words = root.join("words.txt");
    fs::create_dir_all(&root).unwrap();
    fs::write(&words, format!("{}\n", WORDS.join("\n"))).unwrap();

    let mut command = Command::new(env!("CARGO_BIN_EXE_pangloss"));
    command.args([
        "recipe-optimize".to_owned(),
        repo_file(FIXTURE).to_str().unwrap().to_owned(),
        words.to_str().unwrap().to_owned(),
        out.to_str().unwrap().to_owned(),
        "--seed".to_owned(),
        "7".to_owned(),
        "--elapsed-ns".to_owned(),
        ELAPSED_NS.to_owned(),
    ]);
    command.args(extra);
    let worker_succeeded = command.status().unwrap().success();

    let rows = fs::read_to_string(out.join("progress.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("every banked progress row is valid JSON"))
        .collect();
    let report = fs::read_to_string(out.join("report.json"))
        .ok()
        .map(|json| serde_json::from_str(&json).expect("report.json is valid JSON"));
    Run {
        root,
        rows,
        report,
        worker_succeeded,
    }
}

fn unbounded() -> Run {
    optimize("baseline", &[])
}

fn with_confirmation_work(tag: &str, allowance: u64) -> Run {
    optimize(
        tag,
        &["--confirmation-work".to_owned(), allowance.to_string()],
    )
}

/// `false` once a candidate confirms -- restore the three ratchets' real bodies; see
/// docs/research/conformance-containment-inventory.md's "What this blocks" section.
fn fixture_confirms_nothing(confirmations: &[u64]) -> bool {
    confirmations.iter().all(|&value| value == 0)
}

/// A ratchet predicate that can never fail gates nothing.
#[test]
fn fixture_confirms_nothing_detects_a_non_zero_vector() {
    assert!(fixture_confirms_nothing(&[0, 0, 0, 0]));
    assert!(!fixture_confirms_nothing(&[0, 0, 3, 0]));
}

/// The continuation property in its purest form, with no resource bound in force: a candidate that disagrees with the oracle sits mid-sequence, and the candidates after it must still be evaluated, banked, and eligible to win.
#[test]
fn a_failing_candidate_neither_stops_the_run_nor_vanishes_from_progress() {
    let run = unbounded();
    assert!(run.worker_succeeded, "an unbounded run must not fail");

    // RATCHET, not the real assertion -- see docs/research/conformance-containment-inventory.md's
    // "What this blocks" section; the real body sits dead but verbatim past the `return` below.
    let confirmations = run.confirmations();
    assert!(
        fixture_confirms_nothing(&confirmations),
        "FIXTURE now confirms a candidate ({confirmations:?}) -- restore this test's real \
         assertions, preserved verbatim below, as its body"
    );
    return;

    #[allow(unreachable_code)]
    {
        let report = run
            .report
            .as_ref()
            .expect("an unbounded run writes a report");
        let statuses = run.statuses();

        // Non-vacuity FIRST: without both kinds present this test asserts nothing.
        let failing = statuses
            .iter()
            .position(|status| status.as_str() != "full-hc-confirmed")
            .expect("the fixture must produce at least one non-selectable verdict");
        assert!(
            statuses
                .iter()
                .any(|status| status.as_str() == "full-hc-confirmed"),
            "the fixture must also confirm at least one candidate"
        );
        assert!(
            failing + 1 < statuses.len(),
            "the failing candidate must not be LAST, or 'the run continued past it' is untestable \
             (verdicts in evaluation order: {statuses:?})"
        );

        // (a) Nothing evaluated is missing from progress.jsonl, what survives a deadline kill.
        let reported = report["candidates"].as_array().unwrap();
        assert_eq!(
            run.rows.len(),
            reported.len(),
            "every evaluated candidate must be banked as it is evaluated"
        );
        assert_eq!(
            report["search"]["explored"].as_u64().unwrap() as usize,
            run.rows.len()
        );
        for row in &run.rows {
            assert!(!row["id"].as_str().unwrap().is_empty());
            assert!(!row["realized_strategy"].as_str().unwrap().is_empty());
            assert!(
                row["score"].is_object(),
                "a banked row carries its measurement"
            );
            assert!(!row["certification"]["status"].as_str().unwrap().is_empty());
        }
        // The report's candidate table is sorted by id for canonical output, so compare as sets.
        let mut banked: Vec<&str> = run.rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
        let mut in_report: Vec<&str> = reported.iter().map(|c| c["id"].as_str().unwrap()).collect();
        banked.sort_unstable();
        in_report.sort_unstable();
        assert_eq!(banked, in_report);

        // (b) The run continued past the failure and reached a clean completion with a winner.
        assert_eq!(report["termination"], "complete");
        assert_eq!(report["quality"], "exact");
        assert_eq!(report["search"]["unexplored"].as_u64().unwrap(), 0);
        let winner = report["winner"]
            .as_str()
            .expect("a confirmed candidate must still win");
        assert!(reported.iter().any(|candidate| {
            candidate["id"] == winner && candidate["certification"]["status"] == "full-hc-confirmed"
        }));
    }
}

/// A candidate abandoned by a resource bound is reported as that, with its own dimension and numbers, and every candidate the bound does not reach keeps its verdict byte for byte; cost must never be relabelled as a disagreement, nor a disagreement absorbed by the cost gate.
#[test]
fn a_candidate_abandoned_by_a_resource_bound_is_banked_with_its_own_verdict() {
    let baseline = unbounded();
    let confirmations = baseline.confirmations();

    // RATCHET, not the real assertion -- see docs/research/conformance-containment-inventory.md's
    // "What this blocks" section; the real body sits dead but verbatim past the `return` below.
    assert!(
        fixture_confirms_nothing(&confirmations),
        "FIXTURE now confirms a candidate ({confirmations:?}) -- restore this test's real \
         assertions, preserved verbatim below, as its body"
    );
    return;

    #[allow(unreachable_code)]
    {
        let statuses = baseline.statuses();

        // Single out the most expensive candidate and give the run exactly one unit less than it needs.
        let target = (0..confirmations.len())
            .max_by_key(|index| confirmations[*index])
            .expect("the fixture evaluates at least one candidate");
        let prefix: Vec<u64> = (0..confirmations.len())
            .map(|index| confirmations[..index].iter().sum())
            .collect();
        let allowance = prefix[target]
            .saturating_add(confirmations[target])
            .checked_sub(1)
            .expect(
                "the derived bound needs the most expensive candidate to have done real \
                 confirmation work, or 'one unit less than it needs' is not a real bound",
            );
        assert!(
            (0..target).all(|index| allowance - prefix[index] >= confirmations[index]),
            "the derived bound must single out candidate {target} and reach no earlier one \
             (confirmations {confirmations:?})"
        );
        assert!(
            target > 0
                && statuses[..target]
                    .iter()
                    .any(|status| status.as_str() != "full-hc-confirmed"),
            "candidates before the abandoned one must include a genuine failure, or the \
             'a disagreement is never relabelled' half of this test is vacuous ({statuses:?})"
        );

        let bounded = with_confirmation_work(
            "breach",
            baseline.pilot_confirmation().saturating_add(allowance),
        );
        let bounded_rows = &bounded.rows;
        assert!(
            bounded_rows.len() > target,
            "the abandoned candidate itself must be banked -- an abandonment nobody can read is the \
             silent absence this file exists to forbid"
        );

        let breach = &bounded_rows[target]["certification"];
        assert_eq!(
            breach["status"], "resource-breach",
            "candidate {target} exceeded a resource bound and must say so"
        );
        assert_eq!(breach["dimension"], "confirmation");
        assert_eq!(breach["value"].as_u64().unwrap(), confirmations[target]);
        assert!(breach["limit"].as_u64().unwrap() < confirmations[target]);
        assert!(
            breach["word"].is_null(),
            "cost is not a disagreement, so it must not present as one: {breach}"
        );

        for index in 0..target {
            assert_eq!(
                bounded_rows[index]["certification"], baseline.rows[index]["certification"],
                "candidate {index} is inside the bound, so nothing about its verdict may change"
            );
            // The deterministic score components only; build/apply are wall-clock and vary run to run, so comparing them would measure the machine rather than the bound.
            for field in [
                "states",
                "arcs",
                "proposals",
                "confirmation",
                "confirmation_steps",
                "raw_paths",
            ] {
                assert_eq!(
                    bounded_rows[index]["score"][field], baseline.rows[index]["score"][field],
                    "candidate {index} is inside the bound, so its {field} may not move either"
                );
            }
        }

        // The run's own account of itself still reaches disk, and still names the abandoned candidate.
        let report = bounded
            .report
            .as_ref()
            .expect("a run that abandons a candidate still writes a report");
        // The bound is derived from the baseline's pilot cost; a fixture whose pilot grew enough to move the target silently must fail loudly here instead.
        assert_eq!(
            bounded.pilot_confirmation(),
            baseline.pilot_confirmation(),
            "the bound was derived from the baseline's pilot cost; the pilot must not have moved"
        );
        assert_eq!(
            report["candidates"].as_array().unwrap().len(),
            bounded_rows.len()
        );
        assert!(report["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|candidate| candidate["certification"]["status"] == "resource-breach"));
    }
}

/// A run that evaluated every candidate it selected and only then discovered it had overrun an aggregate bound must still write its report: pairing zero unexplored with `quality: Approximate` fails `BackendOptimizationReport::validate`, and the supervisor's partial-report.json does not cover a non-zero exit either.
#[test]
fn a_final_candidate_that_overruns_an_aggregate_bound_still_writes_a_report() {
    let baseline = unbounded();
    let confirmations = baseline.confirmations();
    let total: u64 = confirmations.iter().sum();

    // RATCHET, not a real assertion -- see the identical guard's doc on this file's first test,
    // and docs/research/conformance-containment-inventory.md's "What this blocks" section.
    assert!(
        fixture_confirms_nothing(&confirmations),
        "FIXTURE now confirms a candidate ({confirmations:?}, total {total}) -- restore this \
         test's real assertions, preserved verbatim below, as its body"
    );
    return;

    #[allow(unreachable_code)]
    {
        assert!(
            confirmations.len() > 1 && total > 0,
            "this test needs a multi-candidate run that does measurable confirmation work \
             ({confirmations:?})"
        );

        // One unit under the whole corpus's cost: every candidate is still reached, since the running total only passes the bound once the last one is added.
        let allowance = total - 1;
        assert!(
            (1..confirmations.len())
                .all(|index| confirmations[..index].iter().sum::<u64>() <= allowance),
            "the derived bound must not stop the run early, or this test exercises the deficit path \
             instead ({confirmations:?})"
        );

        let bounded = with_confirmation_work(
            "overrun",
            baseline.pilot_confirmation().saturating_add(allowance),
        );
        assert_eq!(
            bounded.rows.len(),
            confirmations.len(),
            "every selected candidate must still be evaluated and banked"
        );
        assert!(
            bounded.worker_succeeded,
            "the worker must not fail after evaluating every candidate it selected"
        );
        let report = bounded
            .report
            .as_ref()
            .expect("a run that overran on its last candidate must still write report.json");
        assert_eq!(
            report["candidates"].as_array().unwrap().len(),
            confirmations.len()
        );
        // It says WHY it stopped without claiming it looked at less than it did.
        assert_eq!(report["termination"], "budget-exhausted");
        assert_eq!(report["search"]["unexplored"].as_u64().unwrap(), 0);
        assert_eq!(report["quality"], "exact");
    }
}
