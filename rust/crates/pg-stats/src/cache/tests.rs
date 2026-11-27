use super::*;
use crate::model::Direction;
use crate::test_support::TempDir;
use std::io::Write;
use std::sync::{Arc, Barrier};
use std::thread;

pub(super) fn sample_word(form: &str) -> WordRecord {
    WordRecord {
        gates: Some(Vec::new()),
        form: form.to_string(),
        elapsed_ns: 1_000,
        work: 37,
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
            work_steps: 0,
            outputs: 2,
            not_applied: 1,
            no_root: 0,
            surface_mismatch: 0,
            uses: 1,
            self_time_ns: 0,
        }],
    }
}

fn finite(n: u64) -> WorkCap {
    WorkCap::Finite(std::num::NonZeroU64::new(n).unwrap())
}

pub(super) fn sample_run() -> RunMetadata {
    RunMetadata {
        build_info: "test-build".to_string(),
        fwdata_path: "C:/x/project.fwdata".to_string(),
        grammar_hash: "hash-a".to_string(),
        engine: "hc".to_string(),
        options_hash: "opts-a".to_string(),
        options_json: "{}".to_string(),
        created_utc: "2026-08-22T00:00:00Z".to_string(),
        work_cap: None,
    }
}

#[test]
fn streamed_run_matches_slice_flush_row_for_row_in_both_schemas() {
    for version in [schema::SCHEMA_VERSION, schema::COMPACT_SCHEMA_VERSION] {
        let open = || {
            StatsCache::open_on_version(Connection::open_in_memory().unwrap(), "hash-a", version)
                .unwrap()
                .cache
        };
        let mut baseline = open();
        let mut observed = sample_word("apu");
        observed.facts[0].allomorph = Some(StructuralLocator::new("allo-a", "Allomorph A"));
        observed.facts[0].morpheme = Some(StructuralLocator::new("morph-a", "Morpheme A"));
        observed.gates = Some(vec![GateRecord {
            object_key: "entry-a".into(),
            object_kind: ObjectKind::LexEntry,
            object_label: "Entry A".into(),
            identity_quality: IdentityQuality::Authored,
            morpheme: Some(StructuralLocator::new("morpheme-a", "Morpheme A")),
            feature_key: "exception-guid".into(),
            feature_kind: "mpr".into(),
            feature_label: "Exception".into(),
            mode: "excluded".into(),
            passed: 7,
            blocked: 3,
        }]);
        let mut unmeasured = sample_word("apa");
        unmeasured.gates = None;
        let words = vec![observed.clone(), unmeasured, observed];
        baseline.flush(&sample_run(), &words).unwrap();
        let counts: (u64, u64) = baseline
            .connection()
            .query_row("SELECT passed,blocked FROM gate_fact", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(counts, (7, 3));
        let mut writer = open().into_run(&sample_run()).unwrap();
        for word in &words {
            writer.write_word(word).unwrap();
            let stored: u64 = writer
                .cache
                .connection()
                .query_row("SELECT COUNT(*) FROM fact", [], |r| r.get(0))
                .unwrap();
            assert!(stored > 0, "facts must reach SQLite before finish");
        }
        let (streamed, _) = writer.finish().unwrap();
        assert_same_rows(&baseline, &streamed);
        for capacity in [1, 4 * 1024 * 1024] {
            let mut buffer = StatsRunBuffer::with_capacity(capacity);
            for word in &words {
                buffer.write_word(word).unwrap();
            }
            buffer.encoder().unwrap().flush().unwrap();
            assert_eq!(
                buffer.encoder().unwrap().get_ref().get_ref().is_rolled(),
                capacity == 1
            );
            let (buffered, _) = buffer.flush(open(), &sample_run()).unwrap();
            assert_same_rows(&baseline, &buffered);
        }
    }
}

fn assert_same_rows(baseline: &StatsCache, streamed: &StatsCache) {
    for table in [
        "run",
        "word",
        "object",
        "stratum",
        "allomorph",
        "morpheme",
        "fact",
        "feature",
        "gate_fact",
        "cache_identity",
    ] {
        let read = |cache: &StatsCache| {
            let mut statement = cache
                .connection()
                .prepare(&format!("SELECT * FROM {table} ORDER BY 1"))
                .unwrap();
            let columns = statement.column_count();
            statement
                .query_map([], |r| {
                    (0..columns)
                        .map(|i| r.get::<_, rusqlite::types::Value>(i))
                        .collect::<Result<Vec<_>, _>>()
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(read(baseline), read(streamed), "{table}");
    }
}

#[test]
fn atomic_chunks_keep_run_ids_totals_and_last_observation_in_order() {
    let open = || StatsCache::open_in_memory("hash-a").unwrap().cache;
    let chunk = NonZeroUsize::new(2).unwrap();
    let mut again = sample_word("a");
    again.elapsed_ns = 73;
    again.facts[0].attempts = 11;
    let words = [
        sample_word("a"),
        sample_word("b"),
        again,
        sample_word("c"),
        sample_word("d"),
    ];
    for n in 0..=words.len() {
        let mut direct = open();
        if n == 0 {
            direct.flush(&sample_run(), &[]).unwrap();
        }
        for group in words[..n].chunks(chunk.get()) {
            direct.flush(&sample_run(), group).unwrap();
        }
        for capacity in [1, 4 * 1024 * 1024] {
            let mut buffer = StatsRunBuffer::with_capacity(capacity);
            for word in &words[..n] {
                buffer.write_word(word).unwrap();
            }
            let (cache, last_id) = buffer
                .seal()
                .unwrap()
                .replay(open(), &sample_run(), Some(chunk))
                .unwrap();
            assert_eq!(last_id, n.div_ceil(chunk.get()).max(1) as i64);
            assert_same_rows(&direct, &cache);
        }
    }
}

#[test]
fn staged_and_replayed_chunks_preserve_concurrent_ids_labels_and_replacements() {
    let group = NonZeroUsize::new(2).unwrap();
    for version in [schema::SCHEMA_VERSION, schema::COMPACT_SCHEMA_VERSION] {
        for prior in ["none", "before", "during"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("cache.sqlite");
            let open = || {
                StatsCache::open_with_version(&path, "hash-a", version)
                    .unwrap()
                    .cache
            };
            let mut direct = StatsCache::open_on_version(
                Connection::open_in_memory().unwrap(),
                "hash-a",
                version,
            )
            .unwrap()
            .cache;
            let mut seed = sample_word("duplicate");
            seed.facts[0].object_label = "First label".into();
            let seed_words = [sample_word("prior"), seed];
            if prior != "none" {
                direct.flush(&sample_run(), &seed_words).unwrap();
            }
            if prior == "before" {
                open().flush(&sample_run(), &seed_words).unwrap();
            }
            let mut stage =
                staged::BatchWriter::new(open(), sample_run(), Some(group), |_| Ok(())).unwrap();
            let mut large = sample_word("middle");
            large.facts[0].object_key = "large".repeat(4096);
            large.facts[0].object_label = "Spilled label".into();
            let mut replaced = sample_word("duplicate");
            replaced.work += 1;
            let words = [sample_word("duplicate"), large, replaced];
            stage.write_word(words[0].clone()).unwrap();
            assert_eq!(stage.spilled_bytes(), 0);
            stage.limit_raw_queue(1);
            stage.write_word(words[1].clone()).unwrap();
            let spill = stage.spilled_bytes();
            assert!(spill > 16 * 1024);
            stage.limit_raw_queue(1024 * 1024);
            stage.write_word(words[2].clone()).unwrap();
            assert_eq!(stage.spilled_bytes(), spill);
            if prior == "during" {
                open().flush(&sample_run(), &seed_words).unwrap();
            }
            for chunk in words.chunks(group.get()) {
                direct.flush(&sample_run(), chunk).unwrap();
            }
            let (result, _) = stage.finish().unwrap();
            assert_same_rows(&direct, &result);
        }
    }
}

#[test]
fn spill_file_is_truncated_and_offsets_reused_without_mixing_words() {
    let group = NonZeroUsize::new(2).unwrap();
    for cap in [1, 2_000] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.sqlite");
        let mut direct = StatsCache::open_on_version(
            Connection::open_in_memory().unwrap(),
            "hash-a",
            schema::SCHEMA_VERSION,
        )
        .unwrap()
        .cache;
        let open = StatsCache::open(&path, "hash-a").unwrap().cache;
        let mut stage =
            staged::BatchWriter::new(open, sample_run(), Some(group), |_| Ok(())).unwrap();
        stage.limit_raw_queue(1);
        stage.limit_spill(cap);
        let words: Vec<_> = (0..12)
            .map(|i| {
                let mut word = sample_word(&format!("w{i}"));
                word.work += i;
                word.facts[0].object_key = format!("key-{i}-{}", "x".repeat(300));
                word.facts[0].object_label = format!("Label {i}");
                word
            })
            .collect();
        let mut high_water = 0;
        for word in &words {
            stage.write_word(word.clone()).unwrap();
            high_water = high_water.max(stage.spilled_bytes());
        }
        let total: u64 = stage.spilled_bytes();
        // Truncation must have kept the file below the unbounded total.
        assert!(
            high_water < 12 * 300 && total <= high_water,
            "cap {cap}: {high_water}"
        );
        for chunk in words.chunks(group.get()) {
            direct.flush(&sample_run(), chunk).unwrap();
        }
        let (result, _) = stage.finish().unwrap();
        assert_same_rows(&direct, &result);
        let forms: Vec<String> = result
            .conn
            .prepare("SELECT form FROM word ORDER BY rowid")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let expected: Vec<String> = (0..12).map(|i| format!("w{i}")).collect();
        assert_eq!(forms, expected, "cap {cap}");
    }
}

#[test]
fn final_checkpoint_waits_for_a_reader_instead_of_failing_the_batch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.sqlite");
    let mut cache = StatsCache::open_batch(&path, "hash-a", schema::StatsProfile::Full, false)
        .unwrap()
        .cache;
    cache.flush(&sample_run(), &[sample_word("first")]).unwrap();
    // A snapshot older than the last commit blocks a TRUNCATE checkpoint.
    let reader = Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let before: i64 = reader
        .query_row("SELECT count(*) FROM word", [], |row| row.get(0))
        .unwrap();
    assert_eq!(before, 1);
    cache
        .flush(&sample_run(), &[sample_word("second")])
        .unwrap();
    let release = thread::spawn(move || {
        thread::sleep(Duration::from_millis(1500));
        reader.execute_batch("COMMIT").unwrap();
    });
    cache.checkpoint_and_close().unwrap();
    release.join().unwrap();
    let check = Connection::open(&path).unwrap();
    let words: i64 = check
        .query_row("SELECT count(*) FROM word", [], |row| row.get(0))
        .unwrap();
    assert_eq!(words, 2);
}

#[test]
fn abandoned_or_failed_staging_leaves_a_fresh_cache_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.sqlite");
    let open = || StatsCache::open(&path, "hash-a").unwrap().cache;
    for fail in [false, true] {
        let mut stage =
            staged::BatchWriter::new(open(), sample_run(), NonZeroUsize::new(1), |_| Ok(()))
                .unwrap();
        for i in 0..4 {
            stage.write_word(sample_word(&format!("w{i}"))).unwrap();
        }
        if fail {
            let mut bad = sample_word("invalid");
            bad.facts[0].attempts = u64::MAX;
            stage.write_word(bad).unwrap();
            assert!(matches!(
                stage.finish(),
                Err(StatsError::CounterOverflow { .. })
            ));
        } else {
            drop(stage);
        }
        let count: usize = open()
            .conn
            .query_row(
                "SELECT (SELECT count(*) FROM run) + (SELECT count(*) FROM word)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn failed_bulk_copy_rolls_back_every_table_and_engine_claim() {
    for table in ["word", "gate_fact"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.sqlite");
        let open = || StatsCache::open(&path, "hash-a").unwrap().cache;
        let cache = open();
        cache
            .conn
            .execute_batch(&format!("CREATE UNIQUE INDEX reject_copy ON {table} ((1))"))
            .unwrap();
        let mut batch =
            staged::BatchWriter::new(cache, sample_run(), NonZeroUsize::new(1), |_| Ok(()))
                .unwrap();
        for form in ["a", "b"] {
            let mut word = sample_word(form);
            word.gates = Some(vec![GateRecord {
                object_key: "entry-a".into(),
                object_kind: ObjectKind::LexEntry,
                object_label: "Entry A".into(),
                identity_quality: IdentityQuality::Authored,
                morpheme: Some(StructuralLocator::new("morph-a", "Morpheme A")),
                feature_key: "f".into(),
                feature_kind: "k".into(),
                feature_label: "F".into(),
                mode: "required".into(),
                passed: 1,
                blocked: 2,
            }]);
            batch.write_word(word).unwrap();
        }
        let Err(StatsError::Sqlite(error)) = batch.finish() else {
            panic!("expected a constraint failure copying {table}");
        };
        assert!(error.to_string().contains("reject_copy"), "{error}");
        let reopened = open();
        assert_same_rows(
            &StatsCache::open_in_memory("hash-a").unwrap().cache,
            &reopened,
        );
        let integrity: String = reopened
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");
    }
}

#[test]
fn multi_row_inserts_preserve_duplicate_fact_replacement_and_gate_sums() {
    let mut cache = StatsCache::open_in_memory("hash-a").unwrap().cache;
    let mut word = sample_word("w");
    let fact = word.facts[0].clone();
    let gate = GateRecord {
        object_key: fact.object_key.clone(),
        object_label: fact.object_label.clone(),
        object_kind: fact.object_kind,
        identity_quality: fact.identity_quality,
        morpheme: None,
        feature_key: "f".into(),
        feature_kind: "k".into(),
        feature_label: "F".into(),
        mode: "required".into(),
        passed: 2,
        blocked: 1,
    };
    word.facts = (0..INSERT_ROWS + 1)
        .map(|i| FactRecord {
            attempts: i as u64,
            ..fact.clone()
        })
        .collect();
    word.gates = Some(vec![gate; word.facts.len()]);
    cache.flush(&sample_run(), &[word]).unwrap();
    let values: (usize, usize, usize) = cache
        .connection()
        .query_row(
            "SELECT attempts, passed, blocked FROM fact JOIN gate_fact USING(word_id, object_id)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        values,
        (INSERT_ROWS, 2 * (INSERT_ROWS + 1), INSERT_ROWS + 1)
    );
}

#[test]
fn buffered_run_replays_oversized_and_escaped_records_in_order() {
    let open = || {
        StatsCache::open_on_version(
            Connection::open_in_memory().unwrap(),
            "hash-a",
            schema::SCHEMA_VERSION,
        )
        .unwrap()
        .cache
    };
    let mut large = sample_word("large");
    large.facts[0].object_key = "large-rule".into();
    large.facts[0].object_label = "escaped \"label\"\nβ\t".repeat(70_000);
    let words = [sample_word("before"), large, sample_word("after\nβ")];
    let mut direct = open();
    direct.flush(&sample_run(), &words).unwrap();

    let mut buffer = StatsRunBuffer::with_capacity(1);
    for word in &words {
        buffer.write_word(word).unwrap();
    }
    let (buffered, _) = buffer.flush(open(), &sample_run()).unwrap();
    assert_same_rows(&direct, &buffered);
}

#[test]
fn buffered_runs_discard_abandoned_poisoned_and_rejected_observations() {
    let dir = TempDir::new("stats-buffer-rollback");
    let path = dir.path().join("stats.sqlite");
    for capacity in [1, 4 * 1024 * 1024] {
        let mut cache = StatsCache::open(&path, "hash-a").unwrap().cache;
        cache.flush(&sample_run(), &[sample_word("apu")]).unwrap();
        let before = cache
            .connection()
            .query_row("SELECT COUNT(*) FROM run", [], |r| r.get::<_, u64>(0))
            .unwrap();
        let mut replacement = sample_word("apu");
        replacement.passes = 99;
        for failure in [
            "abandon",
            "poison",
            "seal",
            "record",
            "elapsed",
            "decode",
            "truncated string",
            "missing word",
        ] {
            let mut buffer = StatsRunBuffer::with_capacity(capacity);
            buffer.write_word(&replacement).unwrap();
            match failure {
                "abandon" => {
                    drop(buffer);
                }
                "poison" | "seal" => {
                    buffer.records = Some(
                        StatsRunBuffer::encode_spool(tempfile::spooled_tempfile_in(
                            1,
                            dir.path().join("missing"),
                        ))
                        .unwrap(),
                    );
                    let mut big = replacement.clone();
                    if failure == "seal" {
                        buffer.write_word(&big).unwrap();
                        assert!(matches!(
                            buffer.flush(cache, &sample_run()),
                            Err(StatsError::Io(_))
                        ));
                    } else {
                        // Incompressible input forces a write to the missing directory before sealing.
                        let mut state = 1_u32;
                        big.form = (0..2 * 1024 * 1024)
                            .map(|_| {
                                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                                char::from(32 + (state >> 26) as u8)
                            })
                            .collect();
                        assert!(buffer.write_word(&big).is_err());
                        assert!(matches!(
                            buffer.write_word(&replacement),
                            Err(StatsError::RunWriteFailed)
                        ));
                        assert!(matches!(
                            buffer.flush(cache, &sample_run()),
                            Err(StatsError::RunWriteFailed)
                        ));
                    }
                }
                "record" => {
                    let mut bad = sample_word("apa");
                    bad.facts[0].attempts = u64::MAX;
                    buffer.write_word(&bad).unwrap();
                    assert!(matches!(
                        buffer.flush(cache, &sample_run()),
                        Err(StatsError::CounterOverflow { .. })
                    ));
                }
                "elapsed" => {
                    let mut bad = sample_word("apa");
                    bad.elapsed_ns = i64::MAX as u64;
                    buffer.write_word(&bad).unwrap();
                    assert!(matches!(
                        buffer.flush(cache, &sample_run()),
                        Err(StatsError::CounterOverflow {
                            counter: "total_elapsed_ns",
                            ..
                        })
                    ));
                }
                "missing word" => {
                    let mut shortened = StatsRunBuffer::with_capacity(capacity);
                    shortened.write_word(&replacement).unwrap();
                    buffer.write_word(&replacement).unwrap();
                    buffer.records = shortened.records;
                    assert!(matches!(
                        buffer.flush(cache, &sample_run()),
                        Err(StatsError::Io(_))
                    ));
                }
                "decode" | "truncated string" => {
                    if failure == "decode" {
                        buffer.encoder().unwrap().write_all(&[1, 0xff]).unwrap();
                    } else {
                        (2_u64 << 20).encode(buffer.encoder().unwrap()).unwrap();
                        buffer.encoder().unwrap().write_all(&[0; 64]).unwrap();
                    }
                    assert!(matches!(
                        buffer.flush(cache, &sample_run()),
                        Err(StatsError::Io(_))
                    ));
                }
                _ => unreachable!(),
            }
            cache = StatsCache::open(&path, "hash-a").unwrap().cache;
            let passes: u64 = cache
                .connection()
                .query_row("SELECT passes FROM word WHERE form = 'apu'", [], |r| {
                    r.get(0)
                })
                .unwrap();
            let runs: u64 = cache
                .connection()
                .query_row("SELECT COUNT(*) FROM run", [], |r| r.get(0))
                .unwrap();
            let words: u64 = cache
                .connection()
                .query_row("SELECT COUNT(*) FROM word", [], |r| r.get(0))
                .unwrap();
            assert_eq!(passes, 1, "{failure}");
            assert_eq!(runs, before, "{failure}");
            assert_eq!(words, 1, "{failure}");
        }
    }
}

#[test]
fn dropped_and_failed_streamed_runs_roll_back_without_replacing_prior_words() {
    let dir = TempDir::new("stats-stream-rollback");
    let path = dir.path().join("stats.sqlite");
    for fail in [false, true] {
        let mut cache = StatsCache::open(&path, "hash-a").unwrap().cache;
        cache.flush(&sample_run(), &[sample_word("apu")]).unwrap();
        let mut writer = cache.into_run(&sample_run()).unwrap();
        let mut replacement = sample_word("apu");
        replacement.passes = 99;
        writer.write_word(&replacement).unwrap();
        if fail {
            replacement.facts[0].attempts = u64::MAX;
            assert!(writer.write_word(&replacement).is_err());
            assert!(matches!(
                writer.write_word(&sample_word("apa")),
                Err(StatsError::RunWriteFailed)
            ));
            assert!(matches!(writer.finish(), Err(StatsError::RunWriteFailed)));
        } else {
            drop(writer);
        }
        let restored = StatsCache::open(&path, "hash-a").unwrap().cache;
        let passes: u64 = restored
            .connection()
            .query_row("SELECT passes FROM word WHERE form = 'apu'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(passes, 1);
        let runs: u64 = restored
            .connection()
            .query_row("SELECT COUNT(*) FROM run", [], |r| r.get(0))
            .unwrap();
        assert_eq!(runs, if fail { 2 } else { 1 });
    }
}

#[test]
fn streamed_run_refuses_invalid_identity_and_elapsed_overflow() {
    let cache = StatsCache::open_in_memory("hash-a").unwrap().cache;
    let mut run = sample_run();
    run.grammar_hash = "other".into();
    assert!(matches!(
        cache.into_run(&run),
        Err(StatsError::GrammarMismatch { .. })
    ));
    let cache = StatsCache::open_in_memory("hash-a").unwrap().cache;
    let mut writer = cache.into_run(&sample_run()).unwrap();
    let mut word = sample_word("apu");
    word.elapsed_ns = i64::MAX as u64;
    writer.write_word(&word).unwrap();
    word.elapsed_ns = 1;
    assert!(matches!(
        writer.write_word(&word),
        Err(StatsError::CounterOverflow {
            counter: "total_elapsed_ns",
            ..
        })
    ));
    assert!(matches!(writer.finish(), Err(StatsError::RunWriteFailed)));
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
    assert_eq!(per_word[0].work, 37);
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
fn duplicate_fact_keys_replace_all_counters() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let mut word = sample_word("apu");
    word.facts[0].work_steps = 11;
    let mut replacement = word.facts[0].clone();
    replacement.attempts = 7;
    replacement.work_steps = 19;
    word.facts.push(replacement);
    outcome.cache.flush(&sample_run(), &[word]).unwrap();

    let counters: (i64, i64, i64) = outcome
        .cache
        .connection()
        .query_row(
            "SELECT COUNT(*), MAX(attempts), MAX(work_steps) FROM fact",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counters, (1, 7, 19));
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

/// A version-8 cache has no `word.work`; opening it recreates the schema rather than migrating.
#[test]
fn wipes_a_version_eight_cache_whose_word_table_lacks_work() {
    let path = TempDir::new("pg-stats-v8");
    let cache_path = path.path().join("cache.sqlite3");
    let mut first = StatsCache::open(&cache_path, "hash-a").unwrap();
    first
        .cache
        .flush(&sample_run(), &[sample_word("apu")])
        .unwrap();
    drop(first);
    let legacy = Connection::open(&cache_path).unwrap();
    legacy
        .execute_batch(
            "ALTER TABLE word DROP COLUMN work;
             UPDATE cache_identity SET schema_version = 8 WHERE cache_id = 1;
             UPDATE run SET schema_version = 8;",
        )
        .unwrap();
    drop(legacy);

    let mut outcome = StatsCache::open(&cache_path, "hash-a").unwrap();
    assert!(outcome.wiped, "a version-8 cache must be recreated");
    outcome
        .cache
        .flush(&sample_run(), &[sample_word("apu")])
        .unwrap();
    let per_word = crate::report::per_word_report(outcome.cache.connection()).unwrap();
    assert_eq!(per_word.len(), 1);
    assert_eq!(per_word[0].work, 37);
}

#[test]
fn wipes_a_version_seven_cache_whose_stats_keys_predate_compiled_output() {
    let path = TempDir::new("pg-stats-v7");
    let cache_path = path.path().join("cache.sqlite3");
    let first = StatsCache::open(&cache_path, "hash-a").unwrap();
    drop(first);
    let legacy = Connection::open(&cache_path).unwrap();
    legacy
        .execute(
            "UPDATE cache_identity SET schema_version = 7 WHERE cache_id = 1",
            [],
        )
        .unwrap();
    drop(legacy);

    let outcome = StatsCache::open(&cache_path, "hash-a").unwrap();
    assert!(
        outcome.wiped,
        "a version-7 cache must be recreated, not reused under version-8 keys"
    );
    let version: i64 = outcome
        .cache
        .connection()
        .query_row(
            "SELECT schema_version FROM cache_identity WHERE cache_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, crate::schema::SCHEMA_VERSION);
}

/// Schema 10 is the last that stored `run.step_cap` and `word.attempts`; the literal versions are pinned so a bump is a decision.
#[test]
fn a_cache_written_by_schema_ten_is_wiped_not_read() {
    assert_eq!(crate::schema::SCHEMA_VERSION, 13);
    let path = TempDir::new("pg-stats-schema-ten");
    let cache_path = path.path().join("cache.sqlite3");
    let old = Connection::open(&cache_path).unwrap();
    old.execute_batch(
        "CREATE TABLE cache_identity (cache_id INTEGER PRIMARY KEY CHECK (cache_id = 1), schema_version INTEGER NOT NULL, grammar_hash TEXT NOT NULL, engine TEXT);
         CREATE TABLE run (run_id INTEGER PRIMARY KEY, schema_version INTEGER NOT NULL, counter_semantics INTEGER NOT NULL, build_info TEXT NOT NULL, fwdata_path TEXT NOT NULL, grammar_hash TEXT NOT NULL, engine TEXT NOT NULL, options_hash TEXT NOT NULL, options_json TEXT NOT NULL, created_utc TEXT NOT NULL, word_count INTEGER NOT NULL, total_elapsed_ns INTEGER NOT NULL, step_cap INTEGER);
         CREATE TABLE word (word_id INTEGER PRIMARY KEY, run_id INTEGER NOT NULL, form TEXT NOT NULL, elapsed_ns INTEGER NOT NULL, attempts INTEGER NOT NULL, work INTEGER NOT NULL, passes INTEGER NOT NULL, capped INTEGER NOT NULL, timed_out INTEGER NOT NULL, invalid_shape INTEGER NOT NULL);
         INSERT INTO cache_identity VALUES (1, 10, 'hash-a', 'hc');
         INSERT INTO run VALUES (1, 10, 8, 'old', 'f.xml', 'hash-a', 'hc', 'o', '{}', 'unix:0', 1, 1, 200000);
         INSERT INTO word VALUES (1, 1, 'apu', 1, 9, 37, 1, 0, 0, 0);",
    )
    .unwrap();
    drop(old);

    let outcome = StatsCache::open(&cache_path, "hash-a").unwrap();
    assert!(outcome.wiped, "a schema-10 cache must be recreated");
    let words: i64 = outcome
        .cache
        .connection()
        .query_row("SELECT COUNT(*) FROM word", [], |row| row.get(0))
        .unwrap();
    assert_eq!(words, 0);
    let attempts_column = outcome
        .cache
        .connection()
        .prepare("SELECT attempts FROM word")
        .is_ok();
    assert!(
        !attempts_column,
        "the new word table has no attempts column"
    );
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

    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "flush outcomes: {results:?}"
    );
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
fn reopening_at_a_different_work_cap_is_refused_before_any_word_is_skipped() {
    let dir = TempDir::new("pg-stats-work-cap-mismatch");
    let cache_path = dir.path().join("cache.sqlite3");

    let mut first = StatsCache::open(&cache_path, "hash-a").unwrap();
    let mut run_a = sample_run();
    run_a.work_cap = Some(finite(100));
    first.cache.flush(&run_a, &[sample_word("apu")]).unwrap();
    drop(first);

    let mut second = StatsCache::open(&cache_path, "hash-a").unwrap().cache;
    let err = second
        .refuse_if_work_cap_differs(finite(200))
        .expect_err("a cache holding a run at work cap 100 must refuse a work cap 200 report");
    assert!(matches!(err, StatsError::WorkCapMismatch { .. }), "{err}");
    assert!(
        err.to_string().contains("100") && err.to_string().contains("200"),
        "{err}"
    );

    // The same value must never be refused, or every ordinary re-run of `--stats` would break.
    second
        .refuse_if_work_cap_differs(finite(100))
        .expect("the same work cap must not be refused");

    // flush() itself must also refuse, not only the early pg-cli-side check.
    let mut run_b = sample_run();
    run_b.work_cap = Some(finite(200));
    let flush_err = second
        .flush(&run_b, &[sample_word("beta")])
        .expect_err("flush() itself must also refuse a conflicting work cap");
    assert!(
        matches!(flush_err, StatsError::WorkCapMismatch { .. }),
        "{flush_err}"
    );
}

#[test]
fn a_run_that_records_no_work_cap_never_conflicts() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let mut hc_run = sample_run();
    hc_run.work_cap = Some(finite(100));
    outcome.cache.flush(&hc_run, &[sample_word("apu")]).unwrap();

    // A run recording no work cap (e.g. an engine with no such concept) is never a conflict.
    let mut no_cap_run = sample_run();
    no_cap_run.work_cap = None;
    outcome
        .cache
        .flush(&no_cap_run, &[sample_word("beta")])
        .expect("a run recording no work cap must never conflict with one that does");

    outcome
        .cache
        .refuse_if_work_cap_differs(finite(100))
        .expect("a NULL-recording run must not poison the compatibility check");
}

#[test]
fn unbounded_work_cap_round_trips_through_storage() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let mut run = sample_run();
    run.work_cap = Some(WorkCap::Unbounded);
    outcome.cache.flush(&run, &[sample_word("apu")]).unwrap();

    outcome
        .cache
        .refuse_if_work_cap_differs(WorkCap::Unbounded)
        .expect("the same unbounded cap must round-trip and not be refused");
    let err = outcome
        .cache
        .refuse_if_work_cap_differs(finite(100))
        .expect_err("unbounded must still be distinguishable from a finite cap");
    assert!(err.to_string().contains("unbounded"), "{err}");
}

#[test]
fn replacing_word_replaces_all_facts_and_failure_rolls_back() {
    let mut opened = StatsCache::open_in_memory("hash-a").unwrap();
    let mut word = sample_word("apu");
    let mut second = word.facts[0].clone();
    second.object_key = "rule-b".into();
    second.object_label = "Rule B".into();
    word.facts.push(second);
    opened.cache.flush(&sample_run(), &[word.clone()]).unwrap();
    let count = |cache: &StatsCache| {
        cache
            .connection()
            .query_row("SELECT COUNT(*) FROM fact", [], |row| row.get::<_, i64>(0))
            .unwrap()
    };
    assert_eq!(count(&opened.cache), 2);
    word.facts.truncate(1);
    opened.cache.flush(&sample_run(), &[word.clone()]).unwrap();
    assert_eq!(
        count(&opened.cache),
        1,
        "superseded facts must not survive the replacement observation"
    );
    word.facts[0].attempts = u64::MAX;
    assert!(opened.cache.flush(&sample_run(), &[word.clone()]).is_err());
    assert_eq!(
        count(&opened.cache),
        1,
        "failed replacement must restore the prior complete observation"
    );
    word.facts.clear();
    opened.cache.flush(&sample_run(), &[word]).unwrap();
    assert_eq!(
        count(&opened.cache),
        0,
        "an observation with no facts must remove all previous facts"
    );
}

#[test]
fn cached_words_are_reusable_only_under_their_options_and_counter_semantics() {
    let mut opened = StatsCache::open_in_memory("hash-a").unwrap();
    assert!(opened
        .cache
        .reusable_words(&[], "options-a")
        .unwrap()
        .is_empty());

    let mut options_a = sample_run();
    options_a.options_hash = "options-a".into();
    opened
        .cache
        .flush(&options_a, &[sample_word("alpha")])
        .unwrap();

    let mut options_b = sample_run();
    options_b.options_hash = "options-b".into();
    opened
        .cache
        .flush(&options_b, &[sample_word("beta")])
        .unwrap();

    assert_eq!(
        opened
            .cache
            .reusable_words(&["alpha", "beta", "missing"], "options-a")
            .unwrap(),
        ["alpha".to_string()].into_iter().collect()
    );
    assert_eq!(
        opened
            .cache
            .reusable_words(&["alpha", "beta", "missing"], "options-b")
            .unwrap(),
        ["beta".to_string()].into_iter().collect()
    );
    assert_eq!(
        opened
            .cache
            .reusable_words(&["beta", "beta", "missing"], "options-b")
            .unwrap(),
        ["beta".to_string()].into_iter().collect(),
        "duplicate and missing requested forms do not affect owner selection"
    );
    let mut many_missing = (0..33_000)
        .map(|index| format!("missing-{index}"))
        .collect::<Vec<_>>();
    many_missing.extend(["alpha".to_string(), "beta".to_string()]);
    let many_form_refs = many_missing.iter().map(String::as_str).collect::<Vec<_>>();
    assert_eq!(
        opened
            .cache
            .reusable_words(&many_form_refs, "options-a")
            .unwrap(),
        ["alpha".to_string()].into_iter().collect(),
        "large requests are split below SQLite's bind-variable limit"
    );

    let mut overlapping_options = sample_run();
    overlapping_options.options_hash = "options-b".into();
    opened
        .cache
        .flush(&overlapping_options, &[sample_word("alpha")])
        .unwrap();
    assert!(
        opened
            .cache
            .reusable_words(&["alpha"], "options-a")
            .unwrap()
            .is_empty(),
        "the most recent owner run controls reuse for an overlapping word"
    );
    assert_eq!(
        opened
            .cache
            .reusable_words(&["alpha", "beta"], "options-b")
            .unwrap(),
        ["alpha".to_string(), "beta".to_string()]
            .into_iter()
            .collect()
    );

    let beta_run: i64 = opened
        .cache
        .connection()
        .query_row("SELECT run_id FROM word WHERE form = 'beta'", [], |row| {
            row.get(0)
        })
        .unwrap();
    opened
        .cache
        .connection()
        .execute(
            "UPDATE run SET counter_semantics = counter_semantics + 1 WHERE run_id = ?1",
            [beta_run],
        )
        .unwrap();
    assert!(
        opened
            .cache
            .reusable_words(&["beta"], "options-b")
            .unwrap()
            .is_empty(),
        "same options do not make counters reusable under a different counter contract"
    );
}

fn fact_for(
    object_key: &str,
    label: &str,
    morpheme: Option<&str>,
    stratum: &str,
    allomorph: Option<&str>,
) -> FactRecord {
    FactRecord {
        object_key: object_key.to_string(),
        object_label: label.to_string(),
        morpheme: morpheme.map(|m| StructuralLocator::new(m, format!("{m} label"))),
        stratum: Some(StructuralLocator::new(stratum, format!("{stratum} label"))),
        allomorph: allomorph.map(|a| StructuralLocator::new(a, format!("{a} label"))),
        ..sample_word("unused").facts.remove(0)
    }
}

/// Repeated keys with differing later labels and morphemes keep the first label, first-sight ids, and every morpheme named.
#[test]
fn a_flush_interns_each_key_once_in_first_sight_order_with_the_first_label() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let word = |form: &str, facts: Vec<FactRecord>| WordRecord {
        facts,
        ..sample_word(form)
    };
    let words = vec![
        word(
            "one",
            vec![
                fact_for("lex-a", "First", Some("m1"), "s1", Some("a1")),
                fact_for("rule-b", "Rule B", None, "s2", None),
            ],
        ),
        word(
            "two",
            vec![
                fact_for("lex-a", "Second", Some("m2"), "s1", Some("a2")),
                fact_for("lex-a", "Third", Some("m1"), "s2", Some("a1")),
            ],
        ),
    ];
    outcome.cache.flush(&sample_run(), &words).unwrap();
    let conn = outcome.cache.connection();
    let rows = |sql: &str| -> Vec<(i64, String, String)> {
        let mut stmt = conn.prepare(sql).unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let objects = rows("SELECT object_id, key, label FROM object ORDER BY object_id");
    assert_eq!(
        objects
            .iter()
            .map(|o| (o.1.as_str(), o.2.as_str()))
            .collect::<Vec<_>>(),
        [("lex-a", "First"), ("rule-b", "Rule B")]
    );
    assert_eq!(
        rows("SELECT morpheme_id, key, label FROM morpheme WHERE key IS NOT NULL ORDER BY morpheme_id")
            .iter()
            .map(|o| (o.1.as_str(), o.2.as_str()))
            .collect::<Vec<_>>(),
        [("m1", "m1 label"), ("m2", "m2 label")]
    );
    assert_eq!(
        rows(
            "SELECT stratum_id, key, label FROM stratum WHERE key IS NOT NULL ORDER BY stratum_id"
        )
        .iter()
        .map(|o| o.1.as_str())
        .collect::<Vec<_>>(),
        ["s1", "s2"]
    );
    assert_eq!(
        rows("SELECT allomorph_id, key, label FROM allomorph WHERE key IS NOT NULL ORDER BY allomorph_id")
            .iter()
            .map(|o| o.1.as_str())
            .collect::<Vec<_>>(),
        ["a1", "a2"]
    );
    let lex_a_morpheme: String = conn
        .query_row(
            "SELECT m.key FROM object o JOIN morpheme m ON m.morpheme_id = o.morpheme_id WHERE o.key = 'lex-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(lex_a_morpheme, "m1");
    let fact_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM fact", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fact_rows, 4);
}

fn labeled_fact(
    object: (&str, &str),
    morpheme: (&str, &str),
    stratum: (&str, &str),
    allomorph: (&str, &str),
) -> FactRecord {
    FactRecord {
        morpheme: Some(StructuralLocator::new(morpheme.0, morpheme.1)),
        stratum: Some(StructuralLocator::new(stratum.0, stratum.1)),
        allomorph: Some(StructuralLocator::new(allomorph.0, allomorph.1)),
        ..fact_for(object.0, object.1, None, "unused", None)
    }
}

fn single_fact_word(form: &str, fact: FactRecord) -> WordRecord {
    WordRecord {
        facts: vec![fact],
        ..sample_word(form)
    }
}

type Resolved = (String, [(String, String); 4]);

/// Every fact's object, morpheme, stratum and allomorph ids resolved back to (key, label), by word form.
fn resolved_facts(cache: &StatsCache) -> Vec<Resolved> {
    let mut stmt = cache
        .connection()
        .prepare(
            "SELECT w.form, o.key, o.label, m.key, m.label, s.key, s.label, a.key, a.label
             FROM fact f
             JOIN word w ON w.word_id = f.word_id
             JOIN object o ON o.object_id = f.object_id
             JOIN morpheme m ON m.morpheme_id = o.morpheme_id
             JOIN stratum s ON s.stratum_id = f.stratum_id
             JOIN allomorph a ON a.allomorph_id = f.allomorph_id
             ORDER BY w.form",
        )
        .unwrap();
    stmt.query_map([], |r| {
        let pair =
            |i: usize| -> rusqlite::Result<(String, String)> { Ok((r.get(i)?, r.get(i + 1)?)) };
        Ok((r.get(0)?, [pair(1)?, pair(3)?, pair(5)?, pair(7)?]))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn expected(form: &str, pairs: [(&str, &str); 4]) -> Resolved {
    (
        form.to_string(),
        pairs.map(|(key, label)| (key.to_string(), label.to_string())),
    )
}

/// Second flush on one cache: shared keys keep their first labels, new keys are added, and a rolled-back flush leaves no id behind.
#[test]
fn a_later_flush_resolves_every_id_to_its_label_after_a_rollback() {
    let mut opened = StatsCache::open_in_memory("hash-a").unwrap();
    let first = single_fact_word(
        "one",
        labeled_fact(
            ("obj-a", "Obj A"),
            ("m-a", "Morph A"),
            ("s-a", "Stratum A"),
            ("al-a", "Allo A"),
        ),
    );
    opened.cache.flush(&sample_run(), &[first]).unwrap();

    let mut doomed = single_fact_word(
        "doomed",
        labeled_fact(
            ("obj-b", "Doomed"),
            ("m-b", "Doomed"),
            ("s-b", "Doomed"),
            ("al-b", "Doomed"),
        ),
    );
    doomed.facts.push(FactRecord {
        attempts: u64::MAX,
        ..doomed.facts[0].clone()
    });
    assert!(opened.cache.flush(&sample_run(), &[doomed]).is_err());

    let second = [
        single_fact_word(
            "two",
            labeled_fact(
                ("obj-a", "Renamed"),
                ("m-a", "Renamed"),
                ("s-a", "Renamed"),
                ("al-a", "Renamed"),
            ),
        ),
        single_fact_word(
            "three",
            labeled_fact(
                ("obj-b", "Obj B"),
                ("m-b", "Morph B"),
                ("s-b", "Stratum B"),
                ("al-b", "Allo B"),
            ),
        ),
        single_fact_word(
            "four",
            labeled_fact(
                ("obj-b", "Other"),
                ("m-a", "Other"),
                ("s-b", "Other"),
                ("al-a", "Other"),
            ),
        ),
    ];
    opened.cache.flush(&sample_run(), &second).unwrap();

    let a = [
        ("obj-a", "Obj A"),
        ("m-a", "Morph A"),
        ("s-a", "Stratum A"),
        ("al-a", "Allo A"),
    ];
    let b = [
        ("obj-b", "Obj B"),
        ("m-b", "Morph B"),
        ("s-b", "Stratum B"),
        ("al-b", "Allo B"),
    ];
    let four = [
        ("obj-b", "Obj B"),
        ("m-b", "Morph B"),
        ("s-b", "Stratum B"),
        ("al-a", "Allo A"),
    ];
    let mut want = vec![
        expected("one", a),
        expected("two", a),
        expected("three", b),
        expected("four", four),
    ];
    want.sort();
    let mut got = resolved_facts(&opened.cache);
    got.sort();
    assert_eq!(got, want);
}

/// Re-opening a file cache and writing again resolves every id; a wipe on a new grammar restarts ids without reusing the old ones.
#[test]
fn a_reopened_cache_resolves_every_id_to_its_label() {
    let dir = TempDir::new("pg-stats-reopen");
    let path = dir.path().join("cache.sqlite3");
    {
        let mut opened = StatsCache::open(&path, "hash-a").unwrap();
        let first = single_fact_word(
            "one",
            labeled_fact(
                ("obj-a", "Obj A"),
                ("m-a", "Morph A"),
                ("s-a", "Stratum A"),
                ("al-a", "Allo A"),
            ),
        );
        opened.cache.flush(&sample_run(), &[first]).unwrap();
        opened.cache.checkpoint_and_close().unwrap();
    }
    let a = [
        ("obj-a", "Obj A"),
        ("m-a", "Morph A"),
        ("s-a", "Stratum A"),
        ("al-a", "Allo A"),
    ];
    let b = [
        ("obj-b", "Obj B"),
        ("m-b", "Morph B"),
        ("s-b", "Stratum B"),
        ("al-b", "Allo B"),
    ];
    {
        let mut reopened = StatsCache::open(&path, "hash-a").unwrap();
        assert!(!reopened.wiped);
        let more = [
            single_fact_word(
                "two",
                labeled_fact(
                    ("obj-a", "Renamed"),
                    ("m-a", "Renamed"),
                    ("s-a", "Renamed"),
                    ("al-a", "Renamed"),
                ),
            ),
            single_fact_word("three", labeled_fact(b[0], b[1], b[2], b[3])),
        ];
        reopened.cache.flush(&sample_run(), &more).unwrap();
        let mut got = resolved_facts(&reopened.cache);
        got.sort();
        let mut want = vec![expected("one", a), expected("two", a), expected("three", b)];
        want.sort();
        assert_eq!(got, want);
        reopened.cache.checkpoint_and_close().unwrap();
    }
    let mut wiped = StatsCache::open(&path, "hash-b").unwrap();
    assert!(wiped.wiped);
    let run = RunMetadata {
        grammar_hash: "hash-b".to_string(),
        ..sample_run()
    };
    let fresh = [
        single_fact_word("four", labeled_fact(b[0], b[1], b[2], b[3])),
        single_fact_word("five", labeled_fact(a[0], a[1], a[2], a[3])),
    ];
    wiped.cache.flush(&run, &fresh).unwrap();
    let mut got = resolved_facts(&wiped.cache);
    got.sort();
    let mut want = vec![expected("four", b), expected("five", a)];
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn compact_schema_sums_match_full_replay_and_reject_missing_views() {
    let mut full = StatsCache::open_in_memory("hash-a").unwrap().cache;
    let mut compact = StatsCache::open_on_version(
        Connection::open_in_memory().unwrap(),
        "hash-a",
        schema::COMPACT_SCHEMA_VERSION,
    )
    .unwrap()
    .cache;
    let mut word = sample_word("tav");
    word.facts[0].self_time_ns = 23;
    word.facts[0].work_steps = 19;
    let mut named = word.facts[0].clone();
    named.allomorph = Some(StructuralLocator::new("a1", "A1"));
    named.not_applied = 11;
    named.attempts = 3;
    word.facts.push(named.clone());
    named.stratum = Some(StructuralLocator::new("s2", "S2"));
    word.facts.push(named.clone());
    named.direction = Direction::Synthesis;
    word.facts.push(named.clone());
    named.attempts = 7;
    word.facts.push(named); // same full key replaces rather than adds
    let mut other = sample_word("piv");
    other.facts[0].direction = Direction::Synthesis;
    let mut lexical = other.facts[0].clone();
    lexical.object_kind = ObjectKind::LexEntry;
    lexical.object_key = "entry-a".into();
    lexical.morpheme = Some(StructuralLocator::new("morpheme-a", "Morpheme A"));
    other.facts.push(lexical);
    let words = [word, other];
    full.flush(&sample_run(), &words).unwrap();
    compact.flush(&sample_run(), &words).unwrap();
    let sums = |conn: &Connection| {
        let mut stmt = conn.prepare(
            "SELECT w.form, o.key, f.direction, SUM(attempts), SUM(self_time_ns), SUM(work_steps)
             FROM fact f JOIN word w USING(word_id) JOIN object o USING(object_id)
             GROUP BY w.form, o.key, o.kind, f.direction ORDER BY w.form, o.key, f.direction"
        ).unwrap();
        stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    };
    assert_eq!(sums(full.connection()), sums(compact.connection()));
    assert_eq!(
        crate::per_object_report(full.connection(), &Default::default()).unwrap(),
        crate::per_object_report(compact.connection(), &Default::default()).unwrap()
    );
    assert_eq!(
        crate::per_kind_report(full.connection(), &Default::default()).unwrap(),
        crate::per_kind_report(compact.connection(), &Default::default()).unwrap()
    );
    assert_eq!(
        crate::per_morpheme_report(full.connection(), &Default::default()).unwrap(),
        crate::per_morpheme_report(compact.connection(), &Default::default()).unwrap()
    );
    assert!(
        crate::per_allomorph_report(compact.connection(), &Default::default())
            .unwrap_err()
            .to_string()
            .contains("unavailable")
    );
    let filter = crate::PerObjectFilter {
        stratum_key: Some("s2".into()),
        ..Default::default()
    };
    assert!(crate::per_object_report(compact.connection(), &filter)
        .unwrap_err()
        .to_string()
        .contains("unavailable"));
    let pk: Vec<(String, i64)> = compact
        .connection()
        .prepare("PRAGMA table_info(fact)")
        .unwrap()
        .query_map([], |r| Ok((r.get(1)?, r.get(5)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        pk.into_iter().filter(|(_, n)| *n > 0).collect::<Vec<_>>(),
        vec![
            ("word_id".into(), 1),
            ("object_id".into(), 2),
            ("direction".into(), 3)
        ]
    );
    let nonzero_dimensions: i64 = compact
        .connection()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM stratum WHERE stratum_id <> 0) +
                (SELECT COUNT(*) FROM allomorph WHERE allomorph_id <> 0)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(nonzero_dimensions, 0);
    assert!(compact
        .connection()
        .execute("UPDATE fact SET stratum_id = 1", [])
        .is_err());
    assert!(compact
        .connection()
        .execute("UPDATE fact SET allomorph_id = 1", [])
        .is_err());
}

#[test]
fn compact_skips_only_all_zero_rows_and_preserves_words() {
    let mut compact = StatsCache::open_on_version(
        Connection::open_in_memory().unwrap(),
        "hash-a",
        schema::COMPACT_SCHEMA_VERSION,
    )
    .unwrap()
    .cache;
    let mut zero = sample_word("tav");
    let f = &mut zero.facts[0];
    f.attempts = 0;
    f.work = 0;
    f.outputs = 0;
    f.not_applied = 0;
    f.uses = 0;
    let mut words = vec![zero.clone()];
    for counter in 0..9 {
        let mut word = zero.clone();
        word.form = format!("tav{counter}");
        let f = &mut word.facts[0];
        match counter {
            0 => f.attempts = 1,
            1 => f.work = 1,
            2 => f.work_steps = 1,
            3 => f.outputs = 1,
            4 => f.not_applied = 1,
            5 => f.no_root = 1,
            6 => f.surface_mismatch = 1,
            7 => f.uses = 1,
            _ => f.self_time_ns = 1,
        }
        words.push(word);
    }
    compact.flush(&sample_run(), &words).unwrap();
    let counts: (i64, i64) = compact
        .connection()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM word), (SELECT COUNT(*) FROM fact)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (10, 9));
    let mut full = StatsCache::open_in_memory("hash-a").unwrap().cache;
    full.flush(&sample_run(), &words).unwrap();
    assert_eq!(
        full.connection()
            .query_row("SELECT COUNT(*) FROM fact", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        10
    );
}

#[test]
fn compact_reopens_freezes_and_refuses_stale_full_handles() {
    let dir = TempDir::new("compact-reopen");
    let path = dir.path().join("cache.sqlite3");
    let mut full = StatsCache::open(&path, "hash-a").unwrap().cache;
    full.flush(&sample_run(), &[sample_word("tav")]).unwrap();
    let mut compact = StatsCache::open_compact(&path, "hash-a").unwrap();
    assert!(compact.wiped);
    let id = compact
        .cache
        .flush(&sample_run(), &[sample_word("piv")])
        .unwrap();
    assert!(matches!(
        full.flush(&sample_run(), &[sample_word("tav")]),
        Err(StatsError::SchemaMismatch { .. })
    ));
    drop(full);
    compact.cache.checkpoint_and_close().unwrap();
    let snapshot = StatsCache::read_frozen_run(&path, id).unwrap();
    assert_eq!(snapshot.schema_version, schema::COMPACT_SCHEMA_VERSION);
    let reopened = StatsCache::open_compact(&path, "hash-a").unwrap();
    assert!(!reopened.wiped);
    assert!(reopened
        .cache
        .reusable_words(&["piv"], "opts-a")
        .unwrap()
        .contains("piv"));
    drop(reopened);
    assert!(StatsCache::open(&path, "hash-a").unwrap().wiped);
}

#[test]
fn compact_aggregation_overflow_rolls_back() {
    let mut compact = StatsCache::open_on_version(
        Connection::open_in_memory().unwrap(),
        "hash-a",
        schema::COMPACT_SCHEMA_VERSION,
    )
    .unwrap()
    .cache;
    let mut word = sample_word("tav");
    word.facts[0].attempts = i64::MAX as u64;
    let mut other = word.facts[0].clone();
    other.attempts = 1;
    other.allomorph = Some(StructuralLocator::new("a1", "A1"));
    word.facts.push(other);
    assert!(matches!(
        compact.flush(&sample_run(), &[word]),
        Err(StatsError::CounterOverflow {
            counter: "attempts",
            ..
        })
    ));
    assert_eq!(
        compact
            .connection()
            .query_row("SELECT COUNT(*) FROM word", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn gate_facts_replace_on_reparse_and_distinguish_unmeasured_in_both_layouts() {
    for compact in [false, true] {
        let dir = TempDir::new("gate-replace");
        let path = dir.path().join("stats.sqlite");
        let mut cache = if compact {
            StatsCache::open_compact(&path, "hash-a")
        } else {
            StatsCache::open(&path, "hash-a")
        }
        .unwrap()
        .cache;
        let mut word = sample_word("synthetic");
        word.gates = Some(vec![crate::GateRecord {
            object_key: "entry-a".into(),
            object_kind: ObjectKind::LexEntry,
            object_label: "Entry A".into(),
            identity_quality: IdentityQuality::Authored,
            feature_key: "exception-guid".into(),
            morpheme: Some(StructuralLocator::new("morpheme-a", "Morpheme A")),
            feature_kind: "mpr".into(),
            feature_label: "Exception".into(),
            mode: "excluded".into(),
            passed: 7,
            blocked: 3,
        }]);
        cache.flush(&sample_run(), &[word.clone()]).unwrap();
        let counts: (i64, i64, i64) = cache
            .connection()
            .query_row(
                "SELECT gates_measured,passed,blocked FROM word JOIN gate_fact USING(word_id)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(counts, (1, 7, 3));
        let morpheme: String = cache.connection().query_row(
            "SELECT m.key FROM object o JOIN morpheme m USING(morpheme_id) WHERE o.key='entry-a'",
            [], |r| r.get(0),
        ).unwrap();
        assert_eq!(morpheme, "morpheme-a");
        word.gates = Some(Vec::new());
        cache.flush(&sample_run(), &[word.clone()]).unwrap();
        let measured: i64 = cache
            .connection()
            .query_row("SELECT gates_measured FROM word", [], |r| r.get(0))
            .unwrap();
        assert_eq!(measured, 1);
        let remaining: i64 = cache
            .connection()
            .query_row("SELECT COUNT(*) FROM gate_fact", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
        word.gates = None;
        cache.flush(&sample_run(), &[word]).unwrap();
        let measured: i64 = cache
            .connection()
            .query_row("SELECT gates_measured FROM word", [], |r| r.get(0))
            .unwrap();
        assert_eq!(measured, 0);
        assert!(cache
            .reusable_words(&["synthetic"], "opts-a")
            .unwrap()
            .is_empty());
    }
}

fn word_only_record() -> WordRecord {
    WordRecord {
        facts: Vec::new(),
        gates: None,
        ..sample_word("synthetic-01")
    }
}

#[test]
fn word_only_run_freezes_and_reuses_word_measurements_without_fact_storage() {
    let dir = TempDir::new("word-only");
    let path = dir.path().join("cache.sqlite3");
    let cache = StatsCache::open_word_only(&path, "hash-a").unwrap().cache;
    let mut writer = cache.into_run(&sample_run()).unwrap();
    let word = word_only_record();
    writer.write_word(&word).unwrap();
    let (cache, run_id) = writer.finish().unwrap();
    let conn = cache.connection();
    let mut tables = conn
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
        .unwrap();
    let names = tables
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(names, ["cache_identity", "run", "word"]);
    drop(tables);
    assert_eq!(
        conn.query_row("SELECT gates_measured FROM word", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    cache.checkpoint_and_close().unwrap();
    let frozen = StatsCache::read_frozen_run(&path, run_id).unwrap();
    assert_eq!(frozen.schema_version, schema::WORD_ONLY_SCHEMA_VERSION);
    assert_eq!(frozen.words[0].form, word.form);
    assert_eq!(frozen.words[0].elapsed_ns, word.elapsed_ns);
    let reopened = StatsCache::open_word_only(&path, "hash-a").unwrap();
    assert!(!reopened.wiped);
    let cache = reopened.cache;
    assert_eq!(
        cache.reusable_words(&[&word.form], "opts-a").unwrap(),
        HashSet::from([word.form.clone()])
    );
    assert!(cache
        .reusable_words(&[&word.form], "opts-b")
        .unwrap()
        .is_empty());
    assert!(matches!(
        crate::pack::pack(&path),
        Err(StatsError::WordOnlyFactsUnavailable)
    ));
    assert!(!crate::pack::is_packed(cache.connection()).unwrap());
}

#[test]
fn word_only_refuses_unexpected_collector_output_and_rolls_back() {
    let dir = TempDir::new("word-only-write");
    let path = dir.path().join("cache.sqlite3");
    let mut cache = StatsCache::open_word_only(&path, "hash-a").unwrap().cache;
    for word in [
        WordRecord {
            gates: Some(Vec::new()),
            ..word_only_record()
        },
        WordRecord {
            gates: None,
            ..sample_word("synthetic-02")
        },
    ] {
        assert!(matches!(
            cache.flush(&sample_run(), &[word_only_record(), word]),
            Err(StatsError::WordOnlyFactsUnavailable)
        ));
        assert_eq!(
            cache
                .connection()
                .query_row("SELECT COUNT(*) FROM run", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(crate::per_word_report(cache.connection())
            .unwrap()
            .is_empty());
    }
}

#[test]
fn word_only_preserves_populated_legacy_caches() {
    let dir = TempDir::new("legacy-profile");
    for has_identity in [true, false] {
        let path = dir.path().join(format!("legacy-{has_identity}.sqlite"));
        let mut full = StatsCache::open(&path, "hash-a").unwrap().cache;
        full.flush(&sample_run(), &[sample_word("synthetic-01")])
            .unwrap();
        let conn = full.connection();
        conn.execute("UPDATE run SET schema_version=12", [])
            .unwrap();
        if has_identity {
            conn.execute("UPDATE cache_identity SET schema_version=12", [])
                .unwrap();
        } else {
            conn.execute("DROP TABLE cache_identity", []).unwrap();
        }
        assert!(matches!(
            StatsCache::open_word_only(&path, "hash-a"),
            Err(StatsError::ProfileMismatch { .. })
        ));
        assert_eq!(crate::per_word_report(conn).unwrap().len(), 1);
    }
}

#[test]
fn crossing_collection_profiles_refuses_before_unpack_or_wipe() {
    let dir = TempDir::new("profile-mismatch");
    let path = dir.path().join("word.sqlite3");
    let mut cache = StatsCache::open_word_only(&path, "hash-a").unwrap().cache;
    cache.flush(&sample_run(), &[word_only_record()]).unwrap();
    for open_full in [StatsCache::open, StatsCache::open_compact] {
        assert!(matches!(
            open_full(&path, "hash-b"),
            Err(StatsError::ProfileMismatch { .. })
        ));
        let dir = TempDir::new("profile-mismatch-full");
        let full_path = dir.path().join("full.sqlite3");
        let mut full = open_full(&full_path, "hash-a").unwrap().cache;
        full.flush(&sample_run(), &[sample_word("synthetic-01")])
            .unwrap();
        assert!(matches!(
            StatsCache::open_word_only(&full_path, "hash-a"),
            Err(StatsError::ProfileMismatch { .. })
        ));
        assert_eq!(crate::per_word_report(full.connection()).unwrap().len(), 1);
        full.checkpoint_and_close().unwrap();
        crate::pack::pack(&full_path).unwrap();
        let bytes = std::fs::read(&full_path).unwrap();
        assert!(matches!(
            StatsCache::open_word_only(&full_path, "hash-a"),
            Err(StatsError::ProfileMismatch { .. })
        ));
        assert_eq!(bytes, std::fs::read(&full_path).unwrap());
        assert!(crate::pack::is_packed(&Connection::open(&full_path).unwrap()).unwrap());
    }
    assert_eq!(crate::per_word_report(cache.connection()).unwrap().len(), 1);
    cache
        .connection()
        .execute("DELETE FROM cache_identity", [])
        .unwrap();
    assert_eq!(
        crate::collection_profile(cache.connection()).unwrap(),
        crate::StatsProfile::WordOnly
    );
    assert!(matches!(
        StatsCache::open(&path, "hash-a"),
        Err(StatsError::ProfileMismatch { .. })
    ));
}

#[test]
fn scoped_open_reports_its_requested_schema_for_an_unknown_version() {
    let dir = TempDir::new("unknown-profile");
    let path = dir.path().join("cache.sqlite3");
    let cache = StatsCache::open(&path, "hash-a").unwrap().cache;
    let unknown = schema::WORD_ONLY_SCHEMA_VERSION + 1;
    cache
        .connection()
        .execute("UPDATE cache_identity SET schema_version = ?1", [unknown])
        .unwrap();
    cache.checkpoint_and_close().unwrap();
    let before = std::fs::read(&path).unwrap();
    let error = StatsCache::open_word_only(&path, "hash-a").err().unwrap();
    assert!(matches!(
        error,
        StatsError::SchemaMismatch {
            existing,
            requested: schema::WORD_ONLY_SCHEMA_VERSION,
        } if existing == unknown
    ));
    assert_eq!(before, std::fs::read(&path).unwrap());
    assert!(StatsCache::open(&path, "hash-a").unwrap().wiped);
}

#[test]
fn stale_handles_cannot_reuse_or_write_after_external_profile_replacement() {
    let dir = TempDir::new("stale-profile");
    let path = dir.path().join("cache.sqlite3");
    let mut stale = StatsCache::open_word_only(&path, "hash-a").unwrap().cache;
    let conn = Connection::open(&path).unwrap();
    schema::wipe_and_recreate_version(&conn, "hash-a", schema::SCHEMA_VERSION).unwrap();
    let mut full = StatsCache::open(&path, "hash-a").unwrap().cache;
    full.flush(&sample_run(), &[sample_word("synthetic-01")])
        .unwrap();
    assert!(matches!(
        stale.existing_words(&["synthetic-01"]),
        Err(StatsError::SchemaMismatch { .. })
    ));
    assert!(matches!(
        stale.reusable_words(&["synthetic-01"], "opts-a"),
        Err(StatsError::SchemaMismatch { .. })
    ));
    assert!(matches!(
        stale.flush(&sample_run(), &[word_only_record()]),
        Err(StatsError::SchemaMismatch { .. })
    ));
    assert!(matches!(
        stale.intern_stratum(&StructuralLocator::new("id-1", "label")),
        Err(StatsError::SchemaMismatch { .. })
    ));
    assert_eq!(
        crate::per_object_report(full.connection(), &Default::default())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn file_backed_open_enters_wal_journal_mode() {
    let dir = TempDir::new("wal-mode");
    let path = dir.path().join("cache.sqlite3");
    let cache = StatsCache::open(&path, "hash-a").unwrap().cache;
    let mode: String = cache
        .connection()
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}
