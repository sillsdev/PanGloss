//! THE RUN SURVIVES PRODUCING A VERDICT. Pinned end to end, through the real binary.
//!
//! # The gap this file exists to close
//! A per-candidate proposal budget (`--candidate-proposal-work`, `327d559`/`08ac430`/`f228d47`,
//! reverted) shipped with 763 passing tests including a purpose-built gate. Every one of them
//! checked the VERDICT'S SHAPE — that it was typed, that it carried its budget, that it could not be
//! relabelled. Not one checked that the RUN SURVIVED producing it. The reported symptom was the
//! opposite of the feature's purpose: optimizer runs banked FEWER candidates with the bound in force
//! than without it, and a verdict that exists only in a final report that is never written is a
//! silent absence in exactly the case the banking machinery exists for.
//!
//! So the durable property is not "the bound works", it is:
//!
//! > A candidate that ends in a NON-SELECTABLE verdict must (a) appear in `progress.jsonl` as
//! > itself, and (b) leave every other candidate evaluable.
//!
//! That is a property of the optimizer loop, the evaluator, the progress writer, the supervisor and
//! the report validator TOGETHER, which is why these tests drive the real `pangloss` binary rather
//! than `optimize_with_evaluator` directly. Two of the three defects they pin are invisible from
//! inside `pg-foma`: one is a report the validator refuses to accept, the other is a run whose
//! artifacts never reach disk.
//!
//! # Why these bounds and not a per-candidate one
//! There is no per-candidate resource bound reachable from the CLI today — that is precisely what
//! the reverted budget was. `--confirmation-work` is the closest thing: the same number is handed to
//! `RuntimeBudget::confirmation` (a per-candidate post-hoc ceiling that yields
//! `Certification::ResourceBreach`) AND compared against the run's running total by `Budget::admits`.
//! One knob doing double duty means "abandon this candidate" and "end the run" are the SAME event by
//! arithmetic: making candidate k breach requires `allowance - prefix[k] < conf[k]`, and continuing
//! past it requires `prefix[k] + conf[k] <= allowance`, which cannot both hold. A future
//! per-candidate bound has to break that tie, and when it does, the property above is what it owes.
//!
//! # Self-calibration, deliberately
//! Every bound below is computed from an unbounded run of the same fixture in the same test, never
//! hardcoded. `Score`'s confirmation counts are exactly reproducible (see `Score::key`'s doc), so a
//! derived bound is as deterministic as a literal one — and it survives a fixture edit that a
//! literal would silently turn vacuous.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// The staged fixture, chosen because ONE unbounded run of it produces the mix every test here
/// needs: several confirmed candidates, one genuine `identity-mismatch` in the MIDDLE of the
/// sequence, and one candidate whose confirmation work is far above the rest (so a derived bound can
/// single it out). Measured, deterministic, evaluation order:
///
/// | # | realized strategy | confirmation | verdict |
/// |---|---|---|---|
/// | 0-2 | `plan-composed` | 13 each | `full-hc-confirmed` |
/// | 3 | `templated-underlying-tokens` | 11 | `identity-mismatch` |
/// | 4 | `tuned-surface-probed` | 30 | `full-hc-confirmed` |
/// | 5 | `plan-composed` | 13 | `full-hc-confirmed` |
///
/// Nothing below hardcodes those numbers; they are here so a reader knows what the fixture is for.
const FIXTURE: &str = "conformance-staging/edge-cases/recipe-strata-generic/grammar.xml";

/// Verbatim from that fixture's `words.yaml`. Duplicated rather than parsed so the test needs no
/// YAML round trip to produce the plain word list the CLI takes; the non-vacuity assertions in every
/// test below fail loudly if the fixture drifts far enough to matter.
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

/// Generous enough that no assertion here can be decided by the machine's load. These runs measure
/// ~0.3s of work; the deadline exists only so the supervisor has one.
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
    /// `progress.jsonl`, in the order the run banked it. Parsed strictly: an unparseable line is a
    /// failure here, unlike the CLI's own tolerant reader, because these runs are not killed.
    rows: Vec<serde_json::Value>,
    /// `None` when the worker produced no `report.json` at all — which is itself a finding, and one
    /// of the defects this file pins.
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

    /// What the PILOT spent before the search began. `optimize_with_evaluator` is handed
    /// `--confirmation-work` minus this, so a bound derived from per-candidate counts has to add it
    /// back or it lands somewhere else entirely.
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

/// THE continuation property, in its purest form: a candidate that disagrees with the oracle sits in
/// the MIDDLE of the sequence, and the candidates after it must still be evaluated, banked, and
/// eligible to win.
///
/// No resource bound is in force here on purpose. A failing candidate must not truncate the run even
/// when nothing is being rationed — if this cannot hold with an unlimited budget, no bound built on
/// top of it can hold either.
#[test]
fn a_failing_candidate_neither_stops_the_run_nor_vanishes_from_progress() {
    let run = unbounded();
    assert!(run.worker_succeeded, "an unbounded run must not fail");
    let report = run.report.as_ref().expect("an unbounded run writes a report");
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

    // (a) Nothing evaluated is missing from the frontier's own ledger. `progress.jsonl` is what
    //     survives a deadline kill, so a candidate present in the report but absent from it is
    //     exactly the silent absence this file is about.
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
        assert!(row["score"].is_object(), "a banked row carries its measurement");
        assert!(!row["certification"]["status"].as_str().unwrap().is_empty());
    }
    // The report's candidate table is sorted by id for canonical output, so compare as sets.
    let mut banked: Vec<&str> = run.rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
    let mut in_report: Vec<&str> = reported.iter().map(|c| c["id"].as_str().unwrap()).collect();
    banked.sort_unstable();
    in_report.sort_unstable();
    assert_eq!(banked, in_report);

    // (b) The run continued: candidates were evaluated AFTER the failing one, and the run reached a
    //     clean completion with a winner rather than stopping on the failure.
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

/// A candidate abandoned by a resource bound is reported AS THAT, with its own dimension and
/// numbers, and every candidate the bound does not reach keeps its verdict byte for byte.
///
/// Both relabel directions are pinned here, and they are the same two the reverted `f228d47` pinned
/// for `BudgetExceeded` — carried forward onto the bound that actually remains:
///   * cost is never reported as a disagreement (the breaching candidate must not come back as an
///     `identity-mismatch`, which would blame it for being WRONG when it was only expensive), and
///   * a disagreement is never absorbed by the cost gate (the fixture's genuine `identity-mismatch`
///     is inside the bound and must survive it verbatim).
#[test]
fn a_candidate_abandoned_by_a_resource_bound_is_banked_with_its_own_verdict() {
    let baseline = unbounded();
    let confirmations = baseline.confirmations();
    let statuses = baseline.statuses();

    // Single out the most expensive candidate and give the run exactly one unit less than it needs,
    // measured at the point the run reaches it. Derived, never hardcoded -- see this module's doc.
    let target = (0..confirmations.len())
        .max_by_key(|index| confirmations[*index])
        .expect("the fixture evaluates at least one candidate");
    let prefix: Vec<u64> = (0..confirmations.len())
        .map(|index| confirmations[..index].iter().sum())
        .collect();
    let allowance = prefix[target] + confirmations[target] - 1;
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
        // The DETERMINISTIC score components only. `build` and `apply` are wall-clock and vary
        // 15-50% / 6-20% run to run (see `Score::key`'s doc), so comparing them across two
        // invocations would measure the machine rather than the bound.
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
    // The bound is derived from the BASELINE run's pilot cost, and the pilot gets a quarter of the
    // same flag -- so a fixture whose pilot grew enough to breach that quarter would move the target
    // silently. Fail loudly instead.
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

/// A run that evaluated EVERY candidate it selected and only then discovered it had overrun an
/// aggregate bound must still write its report.
///
/// This is the failure mode that loses the most and shows the least: the measured-overrun path
/// leaves `unexplored` at zero by construction — nothing was left unexplored — so a report
/// pairing that with `quality: Approximate` fails
/// [`pg_foma::recipe_report::RecipeOptimizationReport::validate`]'s invariant. A worker that hit
/// this would exit 1 with no `report.json`, and the supervisor's `partial-report.json` does not
/// cover it either (that's written only on a deadline or memory KILL, never a non-zero exit) —
/// losing every already-evaluated, certified, banked candidate, leaving only `progress.jsonl`.
#[test]
fn a_final_candidate_that_overruns_an_aggregate_bound_still_writes_a_report() {
    let baseline = unbounded();
    let confirmations = baseline.confirmations();
    let total: u64 = confirmations.iter().sum();
    assert!(
        confirmations.len() > 1 && total > 0,
        "this test needs a multi-candidate run that does measurable confirmation work \
         ({confirmations:?})"
    );

    // One unit under what the whole corpus of candidates costs: every candidate is still reached
    // (the running total only passes the bound once the LAST one is added), and the overrun is
    // therefore discovered with nothing left unexplored.
    let allowance = total - 1;
    assert!(
        (1..confirmations.len()).all(|index| confirmations[..index].iter().sum::<u64>() <= allowance),
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
