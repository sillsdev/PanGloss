use super::*;
use crate::model::Direction;
use crate::test_support::TempDir;
use std::sync::{Arc, Barrier};
use std::thread;

fn sample_word(form: &str) -> WordRecord {
    WordRecord {
        form: form.to_string(),
        elapsed_ns: 1_000,
        attempts: 10,
        passes: 1,
        capped: false,
        timed_out: false,
        invalid_shape: false,
        facts: vec![FactRecord {
            object_key: "rule-a".to_string(),
            object_kind: ObjectKind::MorphRule,
            object_label: "Rule A".to_string(),
            identity_quality: IdentityQuality::Authored,
            stratum: Some(StructuralLocator::new("0:Root", "Root")),
            allomorph: None,
            morpheme: None,
            direction: Direction::Analysis,
            attempts: 5,
            work: 20,
            outputs: 2,
            not_applied: 1,
            no_root: 0,
            surface_mismatch: 0,
            uses: 1,
            self_time_ns: 0,
        }],
    }
}

fn finite(n: u64) -> StepCap {
    StepCap::Finite(std::num::NonZeroU64::new(n).unwrap())
}

fn sample_run() -> RunMetadata {
    RunMetadata {
        build_info: "test-build".to_string(),
        fwdata_path: "C:/x/project.fwdata".to_string(),
        grammar_hash: "hash-a".to_string(),
        engine: "hc".to_string(),
        options_hash: "opts-a".to_string(),
        options_json: "{}".to_string(),
        created_utc: "2026-08-22T00:00:00Z".to_string(),
        step_cap: None,
    }
}

#[test]
fn create_write_and_read_back_exactly() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    assert!(!outcome.wiped);
    let run = sample_run();
    let words = vec![sample_word("apu")];
    outcome.cache.flush(&run, &words).unwrap();

    let per_word = crate::report::per_word_report(outcome.cache.connection()).unwrap();
    assert_eq!(per_word.len(), 1);
    assert_eq!(per_word[0].form, "apu");
    assert_eq!(per_word[0].attempts, 10);
    assert_eq!(per_word[0].passes, 1);
    assert!(!per_word[0].capped);
    assert!(!per_word[0].timed_out);

    let per_object = crate::report::per_object_report(
        outcome.cache.connection(),
        &crate::report::PerObjectFilter::default(),
    )
    .unwrap();
    assert_eq!(per_object.len(), 1);
    assert_eq!(per_object[0].label, "Rule A");
    assert_eq!(per_object[0].attempts, 5);
    assert_eq!(per_object[0].work, 20);
    assert_eq!(per_object[0].outputs, 2);
    assert_eq!(per_object[0].not_applied, 1);
    assert_eq!(per_object[0].no_root, 0);
    assert_eq!(per_object[0].uses, 1);
}

#[test]
fn wipes_on_grammar_hash_change_and_reports_it() {
    let path = TempDir::new("pg-stats-wipe");
    let cache_path = path.path().join("cache.sqlite3");

    let mut first = StatsCache::open(&cache_path, "hash-a").unwrap();
    assert!(!first.wiped);
    first
        .cache
        .flush(&sample_run(), &[sample_word("apu")])
        .unwrap();

    let second = StatsCache::open(&cache_path, "hash-b").unwrap();
    assert!(
        second.wiped,
        "opening with a different grammar hash must wipe"
    );

    let per_word = crate::report::per_word_report(second.cache.connection()).unwrap();
    assert!(per_word.is_empty(), "old rows must be gone after a wipe");
}

#[test]
fn wipes_an_empty_legacy_cache_before_claiming_current_identity() {
    let path = TempDir::new("pg-stats-empty-legacy");
    let cache_path = path.path().join("cache.sqlite3");
    let legacy = Connection::open(&cache_path).unwrap();
    legacy
        .execute_batch("CREATE TABLE run (run_id INTEGER PRIMARY KEY);")
        .unwrap();
    drop(legacy);

    let outcome = StatsCache::open(&cache_path, "hash-a").unwrap();
    assert!(
        outcome.wiped,
        "an unversioned legacy schema must be replaced"
    );
    let grammar_hash: String = outcome
        .cache
        .connection()
        .query_row(
            "SELECT grammar_hash FROM cache_identity WHERE cache_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(grammar_hash, "hash-a");
}

#[test]
fn no_wipe_on_same_hash_and_accumulates_new_words_only() {
    let path = TempDir::new("pg-stats-accumulate");
    let cache_path = path.path().join("cache.sqlite3");

    let mut first = StatsCache::open(&cache_path, "hash-a").unwrap();
    first
        .cache
        .flush(&sample_run(), &[sample_word("apu")])
        .unwrap();
    drop(first);

    let mut second = StatsCache::open(&cache_path, "hash-a").unwrap();
    assert!(!second.wiped, "same grammar hash must not wipe");

    let already = second.cache.existing_words(&["apu", "beta"]).unwrap();
    assert!(already.contains("apu"));
    assert!(!already.contains("beta"));

    second
        .cache
        .flush(&sample_run(), &[sample_word("beta")])
        .unwrap();

    let per_word = crate::report::per_word_report(second.cache.connection()).unwrap();
    assert_eq!(
        per_word.len(),
        2,
        "both words must be present after accumulation"
    );
    let forms: HashSet<_> = per_word.iter().map(|r| r.form.clone()).collect();
    assert!(forms.contains("apu"));
    assert!(forms.contains("beta"));
}

#[test]
fn mixed_options_hash_is_detected() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let mut run_a = sample_run();
    run_a.options_hash = "opts-a".to_string();
    outcome.cache.flush(&run_a, &[sample_word("apu")]).unwrap();

    let clean = crate::report::mixed_settings(outcome.cache.connection()).unwrap();
    assert!(!clean.is_mixed());

    let mut run_b = sample_run();
    run_b.options_hash = "opts-b".to_string();
    outcome.cache.flush(&run_b, &[sample_word("beta")]).unwrap();

    let mixed = crate::report::mixed_settings(outcome.cache.connection()).unwrap();
    assert!(mixed.is_mixed());
}

#[test]
fn interning_is_stable_across_calls_and_reopen() {
    let path = TempDir::new("pg-stats-intern");
    let cache_path = path.path().join("cache.sqlite3");

    let first = StatsCache::open(&cache_path, "hash-a").unwrap();
    let id_a = first
        .cache
        .intern_object(
            "rule-a",
            ObjectKind::MorphRule,
            "Rule A",
            IdentityQuality::Authored,
            None,
        )
        .unwrap();
    let id_a_again = first
        .cache
        .intern_object(
            "rule-a",
            ObjectKind::MorphRule,
            "Rule A",
            IdentityQuality::Authored,
            None,
        )
        .unwrap();
    assert_eq!(id_a, id_a_again);
    drop(first);

    let second = StatsCache::open(&cache_path, "hash-a").unwrap();
    let id_a_reopened = second
        .cache
        .intern_object(
            "rule-a",
            ObjectKind::MorphRule,
            "Rule A",
            IdentityQuality::Authored,
            None,
        )
        .unwrap();
    assert_eq!(id_a, id_a_reopened);

    let stratum_a = second
        .cache
        .intern_stratum(&StructuralLocator::new("0:Root", "Root"))
        .unwrap();
    let stratum_a_again = second
        .cache
        .intern_stratum(&StructuralLocator::new("0:Root", "Root"))
        .unwrap();
    assert_eq!(stratum_a, stratum_a_again);
}

#[test]
fn stratum_interning_converges_across_separate_connections_to_same_file() {
    let path = TempDir::new("pg-stats-stratum-cross-conn");
    let cache_path = path.path().join("cache.sqlite3");
    let locator = StructuralLocator::new("0:Root", "Root");

    let handle_a = StatsCache::open(&cache_path, "hash-a").unwrap().cache;
    let handle_b = StatsCache::open(&cache_path, "hash-a").unwrap().cache;

    let id_from_a = handle_a.intern_stratum(&locator).unwrap();
    let id_from_b = handle_b.intern_stratum(&locator).unwrap();
    assert_eq!(id_from_a, id_from_b);
}

#[test]
fn allomorph_interning_converges_across_separate_connections_to_same_file() {
    let path = TempDir::new("pg-stats-allomorph-cross-conn");
    let cache_path = path.path().join("cache.sqlite3");
    let locator = StructuralLocator::new("allo-a", "Allo A");

    let handle_a = StatsCache::open(&cache_path, "hash-a").unwrap().cache;
    let handle_b = StatsCache::open(&cache_path, "hash-a").unwrap().cache;

    let id_from_a = handle_a.intern_allomorph(&locator).unwrap();
    let id_from_b = handle_b.intern_allomorph(&locator).unwrap();
    assert_eq!(id_from_a, id_from_b);
}

#[test]
fn huge_counter_round_trips_and_overflow_is_rejected() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let mut word = sample_word("apu");
    word.facts[0].work = i64::MAX as u64;
    outcome.cache.flush(&sample_run(), &[word]).unwrap();

    let per_object = crate::report::per_object_report(
        outcome.cache.connection(),
        &crate::report::PerObjectFilter::default(),
    )
    .unwrap();
    assert_eq!(per_object[0].work, i64::MAX);

    let mut overflowing = sample_word("beta");
    overflowing.facts[0].work = i64::MAX as u64 + 1;
    let err = outcome
        .cache
        .flush(&sample_run(), &[overflowing])
        .unwrap_err();
    assert!(matches!(
        err,
        StatsError::CounterOverflow {
            counter: "work",
            ..
        }
    ));
}

#[test]
fn flush_rejects_a_different_engine_after_the_first_run() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    outcome
        .cache
        .flush(&sample_run(), &[sample_word("apu")])
        .unwrap();
    let mut other = sample_run();
    other.engine = "foma".to_string();
    let err = outcome
        .cache
        .flush(&other, &[sample_word("beta")])
        .expect_err("one cache must not accept facts from two engines");
    assert!(matches!(
        err,
        StatsError::EngineMismatch {
            existing,
            requested
        } if existing == "hc" && requested == "foma"
    ));
    let run_count: i64 = outcome
        .cache
        .connection()
        .query_row("SELECT COUNT(*) FROM run", [], |row| row.get(0))
        .unwrap();
    assert_eq!(run_count, 1, "a rejected flush must write nothing");
}

#[test]
fn concurrent_first_writers_cannot_mix_engines() {
    let dir = TempDir::new("pg-stats-engine-race");
    let cache_path = dir.path().join("cache.sqlite3");
    drop(StatsCache::open(&cache_path, "hash-a").unwrap());

    let barrier = Arc::new(Barrier::new(2));
    let spawn_writer = |engine: &'static str, form: &'static str| {
        let cache_path = cache_path.clone();
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            let mut cache = StatsCache::open(&cache_path, "hash-a").unwrap().cache;
            let mut run = sample_run();
            run.engine = engine.to_string();
            barrier.wait();
            cache.flush(&run, &[sample_word(form)])
        })
    };
    let hc = spawn_writer("hc", "apu");
    let foma = spawn_writer("foma", "beta");
    let results = [hc.join().unwrap(), foma.join().unwrap()];

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(StatsError::EngineMismatch { .. })))
            .count(),
        1
    );
    let cache = StatsCache::open(&cache_path, "hash-a").unwrap().cache;
    let engines: i64 = cache
        .connection()
        .query_row("SELECT COUNT(DISTINCT engine) FROM run", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(engines, 1);
}

#[test]
fn stale_handle_cannot_append_after_another_grammar_recreates_the_cache() {
    let dir = TempDir::new("pg-stats-stale-grammar");
    let cache_path = dir.path().join("cache.sqlite3");
    let mut stale = StatsCache::open(&cache_path, "hash-a").unwrap().cache;
    stale.flush(&sample_run(), &[sample_word("apu")]).unwrap();

    let mut fresh = StatsCache::open(&cache_path, "hash-b").unwrap();
    assert!(fresh.wiped);
    let mut fresh_run = sample_run();
    fresh_run.grammar_hash = "hash-b".to_string();
    fresh
        .cache
        .flush(&fresh_run, &[sample_word("beta")])
        .unwrap();

    let err = stale
        .flush(&sample_run(), &[sample_word("gamma")])
        .expect_err("a stale handle must not append its old grammar after recreation");
    assert!(matches!(err, StatsError::GrammarMismatch { .. }), "{err}");
    let distinct_hashes: i64 = fresh
        .cache
        .connection()
        .query_row("SELECT COUNT(DISTINCT grammar_hash) FROM run", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(distinct_hashes, 1);
}

#[test]
fn concurrent_first_writers_cannot_mix_grammar_hashes() {
    let dir = TempDir::new("pg-stats-grammar-race");
    let cache_path = dir.path().join("cache.sqlite3");
    let cache_a = StatsCache::open(&cache_path, "hash-a").unwrap().cache;
    let cache_b = StatsCache::open(&cache_path, "hash-b").unwrap().cache;
    let barrier = Arc::new(Barrier::new(2));
    let spawn_writer =
        |mut cache: StatsCache, hash: &'static str, form: &'static str, barrier: Arc<Barrier>| {
            thread::spawn(move || {
                let mut run = sample_run();
                run.grammar_hash = hash.to_string();
                barrier.wait();
                cache.flush(&run, &[sample_word(form)])
            })
        };
    let a = spawn_writer(cache_a, "hash-a", "apu", Arc::clone(&barrier));
    let b = spawn_writer(cache_b, "hash-b", "beta", Arc::clone(&barrier));
    let results = [a.join().unwrap(), b.join().unwrap()];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);

    let cache = StatsCache::open(&cache_path, "hash-b").unwrap().cache;
    let distinct_hashes: i64 = cache
        .connection()
        .query_row("SELECT COUNT(DISTINCT grammar_hash) FROM run", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(distinct_hashes, 1);
}

#[test]
fn reopening_at_a_different_step_cap_is_refused_before_any_word_is_skipped() {
    let dir = TempDir::new("pg-stats-step-cap-mismatch");
    let cache_path = dir.path().join("cache.sqlite3");

    let mut first = StatsCache::open(&cache_path, "hash-a").unwrap();
    let mut run_a = sample_run();
    run_a.step_cap = Some(finite(100));
    first.cache.flush(&run_a, &[sample_word("apu")]).unwrap();
    drop(first);

    let mut second = StatsCache::open(&cache_path, "hash-a").unwrap().cache;
    let err = second
        .refuse_if_step_cap_differs(finite(200))
        .expect_err("a cache holding a run at step cap 100 must refuse a step cap 200 report");
    assert!(matches!(err, StatsError::StepCapMismatch { .. }), "{err}");
    assert!(
        err.to_string().contains("100") && err.to_string().contains("200"),
        "{err}"
    );

    // The same value must never be refused, or every ordinary re-run of `--stats` would break.
    second
        .refuse_if_step_cap_differs(finite(100))
        .expect("the same step cap must not be refused");

    // flush() itself must also refuse, not only the early pg-cli-side check.
    let mut run_b = sample_run();
    run_b.step_cap = Some(finite(200));
    let flush_err = second
        .flush(&run_b, &[sample_word("beta")])
        .expect_err("flush() itself must also refuse a conflicting step cap");
    assert!(
        matches!(flush_err, StatsError::StepCapMismatch { .. }),
        "{flush_err}"
    );
}

#[test]
fn a_run_that_records_no_step_cap_never_conflicts() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let mut hc_run = sample_run();
    hc_run.step_cap = Some(finite(100));
    outcome.cache.flush(&hc_run, &[sample_word("apu")]).unwrap();

    // A run recording no step cap (e.g. an engine with no such concept) is never a conflict.
    let mut no_cap_run = sample_run();
    no_cap_run.step_cap = None;
    outcome
        .cache
        .flush(&no_cap_run, &[sample_word("beta")])
        .expect("a run recording no step cap must never conflict with one that does");

    outcome
        .cache
        .refuse_if_step_cap_differs(finite(100))
        .expect("a NULL-recording run must not poison the compatibility check");
}

#[test]
fn unbounded_step_cap_round_trips_through_storage() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let mut run = sample_run();
    run.step_cap = Some(StepCap::Unbounded);
    outcome.cache.flush(&run, &[sample_word("apu")]).unwrap();

    outcome
        .cache
        .refuse_if_step_cap_differs(StepCap::Unbounded)
        .expect("the same unbounded cap must round-trip and not be refused");
    let err = outcome
        .cache
        .refuse_if_step_cap_differs(finite(100))
        .expect_err("unbounded must still be distinguishable from a finite cap");
    assert!(err.to_string().contains("unbounded"), "{err}");
}
