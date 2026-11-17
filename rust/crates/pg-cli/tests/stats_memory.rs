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
        assert!(maxrss_kib < 96 * 1024, "--threads {threads}: {facts} facts retained {maxrss_kib} KiB; stats memory must not grow with the corpus");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn batch_stats_does_not_lock_the_cache_between_words() {
    use std::time::{Duration, Instant};

    struct RunningBatch(std::process::Child);
    impl Drop for RunningBatch {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            let _ = Command::new("kill")
                .args(["-CONT", &self.0.id().to_string()])
                .status();
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("grammar.xml"), grammar(256)).unwrap();
    let words = (0..2048)
        .map(|i| {
            format!(
                "a{}a\n",
                format!("{i:011b}").replace('0', "a").replace('1', "c")
            )
        })
        .collect::<String>();
    fs::write(dir.path().join("words.txt"), words).unwrap();
    fs::write(dir.path().join("one.txt"), "ac\n").unwrap();
    let command = |words: &str, output: &str| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pangloss"));
        cmd.arg("batch")
            .arg(dir.path().join("grammar.xml"))
            .arg(dir.path().join(words))
            .arg(dir.path().join(output))
            .args(["--stats", "--cache"])
            .arg(dir.path().join("cache.sqlite"))
            .args(["--threads", "1", "--work-cap", "unbounded"])
            .stdin(Stdio::null());
        cmd
    };
    let mut first = RunningBatch(
        command("words.txt", "a.tsv")
            .stdout(Stdio::null())
            .stderr(fs::File::create(dir.path().join("a.log")).unwrap())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(
            first.0.try_wait().unwrap().is_none(),
            "first batch exited before overlap"
        );
        if fs::read_to_string(dir.path().join("a.tsv")).is_ok_and(|text| text.lines().count() >= 4)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "first batch did not produce results"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(Command::new("kill")
        .args(["-STOP", &first.0.id().to_string()])
        .status()
        .unwrap()
        .success());
    loop {
        let status = fs::read_to_string(format!("/proc/{}/status", first.0.id())).unwrap();
        if status.lines().any(|line| line.starts_with("State:\tT")) {
            break;
        }
        assert!(Instant::now() < deadline, "first batch did not stop");
        std::thread::sleep(Duration::from_millis(1));
    }
    let conn = rusqlite::Connection::open(dir.path().join("cache.sqlite")).unwrap();
    let runs: u64 = conn
        .query_row("SELECT COUNT(*) FROM run", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        runs, 0,
        "overlap must occur before the first 500-word commit"
    );
    let second = command("one.txt", "b.tsv").output().unwrap();
    assert!(
        second.status.success(),
        "overlapping batch failed while the first was between words: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(Command::new("kill")
        .args(["-CONT", &first.0.id().to_string()])
        .status()
        .unwrap()
        .success());
    assert!(first.0.wait().unwrap().success());
    let words: u64 = conn
        .query_row("SELECT COUNT(*) FROM word", [], |r| r.get(0))
        .unwrap();
    assert_eq!(words, 2049);
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
}
