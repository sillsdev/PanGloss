//! Integration tests for `pg_foma::worker`'s SUPERVISOR half (`run_compile_worker`): these spawn a
//! REAL child process (this crate's own `worker_test_child` test-support binary, `src/bin/
//! worker_test_child.rs`) via `std::process::Command`, exactly as `pg-cli`'s production hidden
//! subcommand will. `worker.rs`'s own `#[cfg(test)] mod tests` already covers the CHILD half
//! (`run_worker_child`) in-process against in-memory buffers; this file is what needs a real
//! spawned process: wall-clock kill timing, and an end-to-end round trip through the real protocol
//! framing over actual OS pipes.
//!
//! Four scenarios (`harden-foma-resource-safety`'s own required test list):
//! 1. A normal compile succeeds through the worker and its result matches the in-process compile.
//! 2. A deliberately-tiny wall timeout is killed and reported as `WallTimeoutKilled` -- not a crash,
//!    not a false success.
//! 3. A real budget trip (the ordering-multiplicity dimension) is reported as `BudgetTripped`.
//! 4. An oversized protocol message is rejected before a child is even spawned (the sharper,
//!    allocation-level version of this same property is worker.rs's own
//!    read_frame_rejects_declared_length_over_limit_before_allocating unit test).

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pg_foma::worker::{
    run_compile_worker, CompileWorkerOutcome, CompileWorkerRequest, GrammarFormat,
    WatchdogEnvelope, WorkerOutcome, V1_WORKER_LIMITS,
};

/// Serializes every test in this file that mutates process-wide environment variables
/// (`PANGLOSS_WORKER_TEST_SLEEP_MS`/`PANGLOSS_WORKER_TEST_CRASH`, read by `worker_test_child`'s own
/// `main`) -- `cargo test` runs tests in this file as threads within one process, so unsynchronized
/// `std::env::set_var` calls across tests would race.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn child_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_worker_test_child"))
}

fn write_grammar(tag: &str, xml: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "pg-foma-worker-supervisor-test-{tag}-{}-{n}.xml",
        std::process::id()
    ));
    std::fs::write(&path, xml).expect("write scratch grammar");
    path
}

/// A synthetic (delanguaged), tiny, clean grammar -- no budget dimension near tripping.
const CLEAN_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>WorkerSupervisorSuccessFixture</Name>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="e1">
            <Allomorphs><Allomorph id="e1-1"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kat</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// A synthetic `Unordered` stratum with 3 loose rules -- exceeding a `ordering_multiplicity_cap` of
/// 2 trips a REAL `ComposeError::OrderingMultiplicityExceeded` through this crate's real production
/// wiring (`FomaProposer::new_with_budget_and_profile`), the same fixture shape `worker.rs`'s own
/// `run_worker_child_reports_budget_tripped_for_a_real_ordering_multiplicity_breach` unit test uses.
const UNORDERED_GRAMMAR_XML: &str = r#"<HermitCrabInput><Language><Name>WorkerSupervisorBudgetTripFixture</Name>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRules="mr1 mr2 mr3" morphologicalRuleOrder="unordered">
      <Name>S</Name>
      <MorphologicalRuleDefinitions>
        <MorphologicalRule id="mr1"><Name>R1</Name><MorphologicalSubrules>
          <MorphologicalSubrule id="s1"><MorphologicalInput><PhoneticSequence id="in1"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="in1" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules></MorphologicalRule>
        <MorphologicalRule id="mr2"><Name>R2</Name><MorphologicalSubrules>
          <MorphologicalSubrule id="s2"><MorphologicalInput><PhoneticSequence id="in2"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="in2" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules></MorphologicalRule>
        <MorphologicalRule id="mr3"><Name>R3</Name><MorphologicalSubrules>
          <MorphologicalSubrule id="s3"><MorphologicalInput><PhoneticSequence id="in3"><SimpleContext naturalClass="ncAll" /></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="in3" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules></MorphologicalRule>
      </MorphologicalRuleDefinitions>
      <LexicalEntries>
        <LexicalEntry id="e1"><Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

/// Scenario 1: a normal compile succeeds through the worker and its result matches the in-process
/// compile (same final state/arc counts from the identical grammar).
#[test]
fn normal_compile_succeeds_through_worker_and_matches_in_process_compile() {
    // Every test in this file spawns a child that inherits this process's environment; the
    // crash/sleep-hook tests mutate that environment for their own duration, so EVERY test here
    // (not only the ones that mutate) must serialize on the same lock to avoid a concurrent thread's
    // env mutation bleeding into this test's own spawned child.
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = write_grammar("normal", CLEAN_XML);
    let request =
        CompileWorkerRequest::new(path.to_string_lossy().into_owned(), GrammarFormat::Xml);
    let envelope = WatchdogEnvelope::default_envelope();

    let outcome = run_compile_worker(&child_exe(), &[], &request, &envelope);
    let (worker_states, worker_arcs) = match outcome {
        WorkerOutcome::Completed(CompileWorkerOutcome::Success {
            final_state_count,
            final_arc_count,
            ..
        }) => (final_state_count, final_arc_count),
        other => panic!("expected Completed(Success), got {other:?}"),
    };

    let grammar = pg_grammar::load(CLEAN_XML).expect("in-process load must succeed");
    let (in_process_result, profile) = pg_foma::analyzer::FomaProposer::new_with_profile(&grammar);
    assert!(
        in_process_result.is_ok(),
        "in-process compile of the identical grammar must succeed"
    );
    assert_eq!(
        worker_states, profile.final_state_count,
        "worker-reported state count must match the in-process compile of the same grammar"
    );
    assert_eq!(
        worker_arcs, profile.final_arc_count,
        "worker-reported arc count must match the in-process compile of the same grammar"
    );

    let _ = std::fs::remove_file(&path);
}

/// Scenario 2: a deliberately-tiny wall timeout kills a deliberately-slow child and is reported as
/// `WallTimeoutKilled` -- not `ChildCrashed`, not a false `Completed(Success)`. Uses
/// `worker_test_child`'s own `PANGLOSS_WORKER_TEST_SLEEP_MS` test-only hook (see that binary's own
/// doc) to make the child provably slower than the armed deadline without needing a real
/// adversarial grammar.
#[test]
fn tiny_wall_timeout_is_killed_and_reported_as_timeout_not_crash_or_false_success() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("PANGLOSS_WORKER_TEST_SLEEP_MS", "5000");

    let path = write_grammar("timeout", CLEAN_XML);
    let request =
        CompileWorkerRequest::new(path.to_string_lossy().into_owned(), GrammarFormat::Xml);
    let envelope =
        WatchdogEnvelope::clamped(Duration::from_millis(200), 4096, Duration::from_millis(20));

    let start = Instant::now();
    let outcome = run_compile_worker(&child_exe(), &[], &request, &envelope);
    let elapsed = start.elapsed();

    std::env::remove_var("PANGLOSS_WORKER_TEST_SLEEP_MS");

    match &outcome {
        WorkerOutcome::WallTimeoutKilled {
            elapsed: reported,
            limit,
        } => {
            assert!(*reported >= envelope.wall_timeout);
            assert_eq!(*limit, envelope.wall_timeout);
        }
        other => panic!("expected WallTimeoutKilled, got {other:?}"),
    }
    assert!(
        elapsed < Duration::from_secs(4),
        "the supervisor must return promptly once it kills the child, not wait out the full 5s \
         sleep (took {elapsed:?})"
    );

    let health = outcome.health_report();
    assert_eq!(health.admission(), pg_foma::health::Severity::Critical);

    let _ = std::fs::remove_file(&path);
}

/// Scenario 3: a real budget trip (the ordering-multiplicity dimension, the one production call
/// site that returns a `ComposeError`-backed failure before ever handing lexc to the foma compiler)
/// is reported as `Completed(BudgetTripped)` through the FULL supervisor round trip (spawn, write
/// request over a real pipe, read result over a real pipe) -- `worker.rs`'s own unit test proves the
/// same mapping in-process; this proves it survives the real IPC framing too.
#[test]
fn budget_trip_is_reported_as_budget_tripped_through_the_full_supervisor_round_trip() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = write_grammar("budget-trip", UNORDERED_GRAMMAR_XML);
    let mut request =
        CompileWorkerRequest::new(path.to_string_lossy().into_owned(), GrammarFormat::Xml);
    request.ordering_multiplicity_cap = Some(2); // fixture has 3 loose rules; must trip.
    let envelope = WatchdogEnvelope::default_envelope();

    let outcome = run_compile_worker(&child_exe(), &[], &request, &envelope);
    match outcome {
        WorkerOutcome::Completed(CompileWorkerOutcome::BudgetTripped { detail, health }) => {
            assert!(detail.contains("ordering-multiplicity"), "detail: {detail}");
            assert_eq!(health.admission(), pg_foma::health::Severity::Critical);
        }
        other => panic!("expected Completed(BudgetTripped), got {other:?}"),
    }

    let _ = std::fs::remove_file(&path);
}

/// Scenario 4: an oversized request is rejected as `ProtocolViolation` before any child is spawned
/// at all (the frame-level, allocation-before-check property is unit-tested more sharply in
/// worker.rs's own read_frame_rejects_declared_length_over_limit_before_allocating, which
/// declares a `u64::MAX` length with zero payload bytes and proves the rejection happens before any
/// attempt to allocate that many bytes; this test proves the supervisor's OWN pre-spawn guard using
/// a real, if wasteful-on-purpose, oversized field).
#[test]
fn oversized_request_is_rejected_as_protocol_violation_before_any_child_is_spawned() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let huge_path = "x".repeat((V1_WORKER_LIMITS.max_request_bytes + 1) as usize);
    let request = CompileWorkerRequest::new(huge_path, GrammarFormat::Xml);
    let envelope = WatchdogEnvelope::default_envelope();

    let outcome = run_compile_worker(&child_exe(), &[], &request, &envelope);
    match outcome {
        WorkerOutcome::ProtocolViolation { .. } => {}
        other => panic!("expected ProtocolViolation, got {other:?}"),
    }
}

/// Bonus (not one of the four required scenarios, but cheap and directly proves
/// `WorkerOutcome::ChildCrashed` is reachable and distinct from `WallTimeoutKilled`/
/// `ProtocolViolation`): a child that exits abnormally before writing any result frame at all is
/// classified as a crash, using `worker_test_child`'s own `PANGLOSS_WORKER_TEST_CRASH` hook.
#[test]
fn abnormal_child_exit_with_no_result_frame_is_reported_as_child_crashed() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("PANGLOSS_WORKER_TEST_CRASH", "1");

    let path = write_grammar("crash", CLEAN_XML);
    let request =
        CompileWorkerRequest::new(path.to_string_lossy().into_owned(), GrammarFormat::Xml);
    let envelope = WatchdogEnvelope::default_envelope();

    let outcome = run_compile_worker(&child_exe(), &[], &request, &envelope);

    std::env::remove_var("PANGLOSS_WORKER_TEST_CRASH");

    match outcome {
        WorkerOutcome::ChildCrashed { .. } => {}
        other => panic!("expected ChildCrashed, got {other:?}"),
    }

    let _ = std::fs::remove_file(&path);
}
