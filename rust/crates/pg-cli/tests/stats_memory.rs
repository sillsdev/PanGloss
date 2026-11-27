use std::fs;
use std::process::{Command, Stdio};

fn grammar(objects: usize) -> String {
    let rules = (0..objects)
        .map(|i| format!("<MorphologicalRule id=\"r{i}\"><Name>r{i}</Name><MorphologicalSubrules><MorphologicalSubrule id=\"s{i}\"><MorphologicalInput><PhoneticSequence id=\"p{i}\"><OptionalSegmentSequence min=\"1\" max=\"-1\"><SimpleContext naturalClass=\"any\"/></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index=\"p{i}\"/><InsertSegments><PhoneticShape>b</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>r{i}</MorphemeId></MorphologicalRule>"))
        .collect::<String>();
    let ids = (0..objects)
        .map(|i| format!("r{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r#"<HermitCrabInput><Language><Name>StatsMemory</Name><PartsOfSpeech><PartOfSpeech id="pos"><Name>N</Name></PartOfSpeech></PartsOfSpeech><CharacterDefinitionTable id="table"><Name>table</Name><SegmentDefinitions><SegmentDefinition id="a"><Representations><Representation>a</Representation></Representations></SegmentDefinition><SegmentDefinition id="b"><Representations><Representation>b</Representation></Representations></SegmentDefinition><SegmentDefinition id="c"><Representations><Representation>c</Representation></Representations></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable><NaturalClasses><SegmentNaturalClass id="any"><Name>any</Name><Segment segment="a"/><Segment segment="b"/><Segment segment="c"/></SegmentNaturalClass></NaturalClasses><Strata><Stratum characterDefinitionTable="table" morphologicalRuleOrder="unordered" morphologicalRules="{ids}"><Name>Root</Name><MorphologicalRuleDefinitions>{rules}</MorphologicalRuleDefinitions><LexicalEntries><LexicalEntry id="root" partOfSpeech="pos"><MorphemeId>root</MorphemeId><Allomorphs><Allomorph id="rootA"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#
    )
}

#[test]
#[cfg(target_os = "linux")]
fn batch_stats_memory_is_bounded_across_the_corpus() {
    let dir = std::env::temp_dir().join(format!("pangloss-stats-memory-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("grammar.xml"), grammar(256)).unwrap();
    let words = (0..2048)
        .map(|i| {
            format!(
                "a{}a\n",
                format!("{i:011b}").replace('0', "a").replace('1', "c")
            )
        })
        .collect::<String>();
    fs::write(dir.join("words.txt"), words).unwrap();
    for (threads, frozen, compact) in [
        ("1", false, false),
        ("4", false, false),
        ("4", true, false),
        ("1", true, true),
    ] {
        let tag = format!("{threads}-{frozen}-{compact}");
        let cache = dir.join(format!("cache-{tag}.sqlite"));
        let rss = dir.join(format!("rss-{tag}.txt"));
        let mut command = Command::new("/usr/bin/time");
        command
            .args(["-f", "%M", "-o"])
            .arg(&rss)
            .arg(env!("CARGO_BIN_EXE_pangloss"))
            .arg("batch")
            .arg(dir.join("grammar.xml"))
            .arg(dir.join("words.txt"))
            .arg(dir.join(format!("out-{tag}.tsv")))
            .args(["--stats", "--cache"])
            .arg(&cache)
            .args([
                "--threads",
                threads,
                "--no-flush",
                "--work-cap",
                "unbounded",
            ]);
        if frozen {
            command
                .arg("--stats-manifest")
                .arg(dir.join(format!("manifest-{tag}.json")));
        }
        if compact {
            command.arg("--stats-compact");
        }
        let output = command.stdin(Stdio::null()).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let conn = rusqlite::Connection::open(&cache).unwrap();
        let runs: u64 = conn
            .query_row("SELECT COUNT(*) FROM run", [], |r| r.get(0))
            .unwrap();
        assert_eq!(runs, if frozen { 1 } else { 5 });
        let facts: u64 = conn
            .query_row("SELECT COUNT(*) FROM fact", [], |r| r.get(0))
            .unwrap();
        assert!(
            facts >= 2048 * 256,
            "the synthetic rules must actually emit facts: {facts}"
        );
        let maxrss_kib: u64 = fs::read_to_string(&rss).unwrap().trim().parse().unwrap();
        let limit_kib = 96 * 1024;
        eprintln!("stats peak RSS: {maxrss_kib} KiB (limit {limit_kib} KiB; threads={threads}, frozen={frozen}, compact={compact})");
        // The 96 MiB limit leaves room for parser and SQLite variance but cannot hold the raw corpus.
        assert!(maxrss_kib < limit_kib, "--threads {threads}: {facts} facts retained {maxrss_kib} KiB; stats memory must not grow with the corpus");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(target_os = "linux")]
mod concurrent {
    use super::{fs, grammar, Command, Stdio};
    use std::os::unix::process::CommandExt;
    use std::path::Path;
    use std::time::{Duration, Instant};

    struct RunningBatch(std::process::Child);

    impl RunningBatch {
        fn signal(&self, signal: &str) {
            assert!(Command::new("kill")
                .args([signal, &self.0.id().to_string()])
                .status()
                .unwrap()
                .success());
        }
    }

    impl Drop for RunningBatch {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn exercise_two_batches(override_policy: bool, hold_publish_lock: bool) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("grammar.xml"), grammar(256)).unwrap();
        let words: String = (0..2048)
            .map(|i| {
                format!(
                    "a{}a\n",
                    format!("{i:011b}").replace('0', "a").replace('1', "c")
                )
            })
            .collect();
        fs::write(dir.path().join("words.txt"), words).unwrap();
        fs::write(dir.path().join("one.txt"), "ac\n").unwrap();
        let cache = dir.path().join("cache.sqlite");
        let command = |words: &str, output: &str| {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_pangloss"));
            cmd.arg("batch")
                .arg(dir.path().join("grammar.xml"))
                .arg(dir.path().join(words))
                .arg(dir.path().join(output))
                .args(["--stats", "--cache"])
                .arg(&cache)
                .args(["--threads", "1", "--work-cap", "unbounded"])
                .stdin(Stdio::null());
            cmd
        };
        let first_log = dir.path().join("a.log");
        let mut first = RunningBatch(
            command("words.txt", "a.tsv")
                .stdout(Stdio::null())
                .stderr(fs::File::create(&first_log).unwrap())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            assert!(
                first.0.try_wait().unwrap().is_none(),
                "first batch exited before overlap"
            );
            if fs::read_to_string(dir.path().join("a.tsv"))
                .is_ok_and(|text| text.lines().count() >= 4)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "first batch did not produce results"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        first.signal("-STOP");
        loop {
            let status = fs::read_to_string(format!("/proc/{}/status", first.0.id())).unwrap();
            if status.lines().any(|line| line.starts_with("State:\tT")) {
                break;
            }
            assert!(Instant::now() < deadline, "first batch did not stop");
            std::thread::sleep(Duration::from_millis(1));
        }
        let conn = rusqlite::Connection::open(&cache).unwrap();
        let counts = || {
            conn.query_row(
                "SELECT (SELECT COUNT(*) FROM run), (SELECT COUNT(*) FROM word),
                        (SELECT COUNT(*) FROM fact), (SELECT COUNT(*) FROM gate_fact)",
                [],
                |row| {
                    Ok((
                        row.get::<_, u64>(0)?,
                        row.get::<_, u64>(1)?,
                        row.get::<_, u64>(2)?,
                        row.get::<_, u64>(3)?,
                    ))
                },
            )
            .unwrap()
        };
        assert_eq!(counts(), (0, 0, 0, 0), "overlap must precede publication");
        let mut second = command("one.txt", "b.tsv");
        if override_policy {
            second.arg("--always-enforce-final-templates");
        }
        let second = second.output().unwrap();
        assert!(
            second.status.success(),
            "overlapping batch failed while the first was between words: {}",
            String::from_utf8_lossy(&second.stderr)
        );
        let published = counts();
        assert_eq!((published.0, published.1), (1, 1));
        if hold_publish_lock {
            conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        }
        first.signal("-CONT");
        if hold_publish_lock {
            let deadline = Instant::now() + Duration::from_secs(120);
            while !fs::read_to_string(&first_log)
                .unwrap()
                .contains("PARSEELAPSED")
            {
                assert!(
                    first.0.try_wait().unwrap().is_none(),
                    "batch exited before publication"
                );
                assert!(Instant::now() < deadline, "batch did not reach publication");
                std::thread::sleep(Duration::from_millis(10));
            }
            // Keep the actual SQLite write lock beyond the original five-second busy timeout.
            std::thread::sleep(Duration::from_secs(6));
            assert!(
                first.0.try_wait().unwrap().is_none(),
                "batch gave up on the write lock"
            );
            conn.execute_batch("COMMIT").unwrap();
        }
        let status = first.0.wait().unwrap();
        let log = fs::read_to_string(&first_log).unwrap();
        if override_policy {
            assert!(!status.success(), "{log}");
            assert!(
                log.contains("records always_enforce_final_templates=true but requested false"),
                "{log}"
            );
            assert_eq!(counts(), published);
        } else {
            assert!(status.success(), "{log}");
            assert_eq!(counts().1, 2049);
        }
        let integrity: String = conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");
    }

    #[test]
    fn batch_stats_does_not_lock_the_cache_between_words() {
        exercise_two_batches(false, false);
    }

    #[test]
    fn competing_stats_policies_cannot_both_publish() {
        exercise_two_batches(true, false);
    }

    #[test]
    fn batch_stats_waits_for_a_publisher_past_the_busy_timeout() {
        exercise_two_batches(false, true);
    }

    fn batch_command(directory: &Path, words: &str, cache: &Path) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pangloss"));
        cmd.arg("batch")
            .arg(directory.join("grammar.xml"))
            .arg(directory.join(words))
            .arg(directory.join("batch.tsv"))
            .args(["--stats", "--cache"])
            .arg(cache)
            .args(["--threads", "1", "--work-cap", "unbounded"])
            .stdin(Stdio::null())
            .stdout(Stdio::null());
        cmd
    }

    #[test]
    fn populated_batch_announces_wait_and_pack_reports_busy() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("grammar.xml"), grammar(1)).unwrap();
        fs::write(dir.path().join("seed.txt"), "a\n").unwrap();
        fs::write(dir.path().join("new.txt"), "ac\n").unwrap();
        let cache = dir.path().join("cache.sqlite");
        let seeded = batch_command(dir.path(), "seed.txt", &cache)
            .output()
            .unwrap();
        assert!(seeded.status.success(), "{:?}", seeded.stderr);
        let conn = rusqlite::Connection::open(&cache).unwrap();
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();

        let log = dir.path().join("waiting.log");
        let mut batch = RunningBatch(
            batch_command(dir.path(), "new.txt", &cache)
                .stderr(fs::File::create(&log).unwrap())
                .spawn()
                .unwrap(),
        );
        let waiting_message = "stats: waiting for the cache write lock";
        let deadline = Instant::now() + Duration::from_secs(15);
        while !fs::read_to_string(&log).unwrap().contains(waiting_message) {
            assert!(batch.0.try_wait().unwrap().is_none());
            assert!(Instant::now() < deadline, "batch never announced its wait");
            std::thread::sleep(Duration::from_millis(10));
        }
        let packed = Command::new(env!("CARGO_BIN_EXE_pangloss"))
            .args(["stats-cache", "pack", "--cache"])
            .arg(&cache)
            .output()
            .unwrap();
        assert!(!packed.status.success());
        assert!(String::from_utf8_lossy(&packed.stderr)
            .contains("stats cache busy: cannot pack while another writer holds the cache"));
        assert!(batch.0.try_wait().unwrap().is_none());
        conn.execute_batch("COMMIT").unwrap();
        assert!(
            batch.0.wait().unwrap().success(),
            "{}",
            fs::read_to_string(&log).unwrap()
        );
        assert_eq!(
            fs::read_to_string(&log)
                .unwrap()
                .matches(waiting_message)
                .count(),
            1
        );
        let words: u64 = conn
            .query_row("SELECT COUNT(*) FROM word", [], |row| row.get(0))
            .unwrap();
        assert_eq!(words, 2);
    }

    #[test]
    fn interrupt_removes_the_fresh_stage_beside_its_destination() {
        let dir = tempfile::tempdir().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("grammar.xml"), grammar(256)).unwrap();
        let words: String = (0..8192)
            .map(|i| {
                format!("a{:013b}a\n", i)
                    .replace('0', "a")
                    .replace('1', "c")
            })
            .collect();
        fs::write(dir.path().join("words.txt"), words).unwrap();
        let cache = dir.path().join("cache.sqlite");
        let mut batch = RunningBatch(
            batch_command(dir.path(), "words.txt", &cache)
                .env("TMPDIR", tmp.path())
                .process_group(0)
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(30);
        let staged_path = loop {
            let staged = fs::read_dir(dir.path())
                .unwrap()
                .map(Result::unwrap)
                .find(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("pangloss-stats-")
                        && entry.metadata().is_ok_and(|meta| meta.len() > 0)
                });
            if let Some(entry) = staged {
                break entry.path();
            }
            assert!(batch.0.try_wait().unwrap().is_none());
            assert!(
                Instant::now() < deadline,
                "private database never reached the cache directory"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(Command::new("kill")
            .args(["-INT", "--", &format!("-{}", batch.0.id())])
            .status()
            .unwrap()
            .success());
        assert!(!batch.0.wait().unwrap().success());
        let deadline = Instant::now() + Duration::from_secs(10);
        while staged_path.exists() {
            assert!(
                Instant::now() < deadline,
                "interrupted stage was left behind"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let conn = rusqlite::Connection::open(&cache).unwrap();
        let words: u64 = conn
            .query_row("SELECT COUNT(*) FROM word", [], |row| row.get(0))
            .unwrap();
        assert_eq!(words, 0);
    }
}
