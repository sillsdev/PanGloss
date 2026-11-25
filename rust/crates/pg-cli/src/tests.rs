//! Tests the CLI parser and batch command integration.
use super::{
    load_grammar, run_batch, run_batch_counted, write_parse_analysis_row, WorkCap, DEFAULT_WORK_CAP,
};
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

mod partials;

/// A minimal, self-contained grammar (no phonological/morphological rules): one stratum, one table, one `LexicalEntry` whose surface form is "kat" — root-only lookup is enough to exercise the batch pipeline.
pub(crate) const MINI_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MiniCliTest</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="seg6"><Name>SegId</Name>
        <Symbols><Symbol id="idA">a</Symbol><Symbol id="idT">t</Symbol><Symbol id="idK">k</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="seg1"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="seg6" symbolValues="idA" /></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations>
          <FeatureValue feature="seg6" symbolValues="idT" /></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations>
          <FeatureValue feature="seg6" symbolValues="idK" /></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="bdry1"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="le1" partOfSpeech="n">
            <Allomorphs><Allomorph id="le1-1"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kat</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// A fresh, collision-free scratch directory per test, since tests in one binary run concurrently by default.
fn scratch_dir(tag: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "pangloss-cli-test-{tag}-{}-{n}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

#[test]
fn fwbackup_extension_routes_through_fieldworks_importer() {
    for extension in ["fwbackup", "FWBACKUP"] {
        let missing = scratch_dir("missing-fwbackup").join(format!("missing.{extension}"));
        let err = load_grammar(&missing.to_string_lossy()).unwrap_err();
        assert!(
            err.starts_with("import "),
            "expected .{extension} to use pg-fwdata import, got: {err}"
        );
    }
}

/// Runs `batch` with `extra_args` appended after the 3 positional args, returning the written TSV's lines.
fn run_batch_tsv(tag: &str, extra_args: &[&str]) -> Vec<String> {
    run_batch_tsv_custom(tag, MINI_GRAMMAR_XML, "kat\n", extra_args)
}

/// Like `run_batch_tsv`, but with a caller-supplied grammar/word-list body, used by the `--guess` tests below whose grammar needs a lexical pattern `MINI_GRAMMAR_XML` doesn't have.
fn run_batch_tsv_custom(
    tag: &str,
    grammar_xml: &str,
    words_text: &str,
    extra_args: &[&str],
) -> Vec<String> {
    let dir = scratch_dir(tag);
    let grammar_path = dir.join("grammar.xml");
    let words_path = dir.join("words.txt");
    let out_path = dir.join("out.tsv");
    fs::write(&grammar_path, grammar_xml).expect("write grammar");
    fs::write(&words_path, words_text).expect("write words");

    let mut args: Vec<String> = vec![
        grammar_path.to_string_lossy().into_owned(),
        words_path.to_string_lossy().into_owned(),
        out_path.to_string_lossy().into_owned(),
    ];
    args.extend(extra_args.iter().map(|s| s.to_string()));

    run_batch(&args).unwrap_or_else(|e| panic!("run_batch failed: {e}"));
    fs::read_to_string(&out_path)
        .expect("read out.tsv")
        .lines()
        .map(str::to_string)
        .collect()
}

const PER_WORD_PROFILE_CHILD_ENV: &str = "PG_TEST_PER_WORD_PROFILE_CHILD";
const PER_WORD_PROFILE_CHILD_VALUE: &str = "alt-yield-effect-v1";
const PER_WORD_PROFILE_CHILD_MARKER: &str = "PG_PER_WORD_PROFILE_EFFECT_ASSERTIONS_PASSED";
const PER_WORD_PROFILE_TEST_NAME: &str =
    "tests::per_word_profile_does_not_accumulate_across_repeated_batch_words";

fn run_batch_alt_yield_snapshot(
    tag: &str,
    words_text: &str,
) -> pg_parse::alt_yield::AltYieldSnapshot {
    let tag = tag.to_string();
    let words_text = words_text.to_string();
    std::thread::spawn(move || {
        let dir = scratch_dir(&tag);
        let grammar_path = dir.join("grammar.xml");
        let words_path = dir.join("words.txt");
        let out_path = dir.join("out.tsv");
        fs::write(&grammar_path, MINI_GRAMMAR_XML).expect("write grammar");
        fs::write(&words_path, words_text).expect("write words");
        run_batch(&[
            grammar_path.to_string_lossy().into_owned(),
            words_path.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
            "--threads".to_string(),
            "1".to_string(),
        ])
        .expect("run batch with per-word profiling");
        pg_parse::alt_yield::snapshot()
    })
    .join()
    .expect("batch profiling thread must complete")
}

fn assert_repeated_batch_word_profile_is_per_word() {
    let once = run_batch_alt_yield_snapshot("alt-yield-once", "kat\n");
    let twice = run_batch_alt_yield_snapshot("alt-yield-twice", "kat\nkat\n");

    assert!(
        once.expanded_total > 0,
        "fixture must exercise alternative expansion"
    );
    assert_eq!(
        twice.canonical_alt_total, once.canonical_alt_total,
        "the second word's canonical count must start at zero"
    );
    assert_eq!(
        twice.canonical_alt_max, once.canonical_alt_max,
        "the second word's canonical maximum must start at zero"
    );
    assert_eq!(
        twice.expanded_total, once.expanded_total,
        "the second word's expansion count must start at zero"
    );
    assert_eq!(
        twice.distinct_identities, once.distinct_identities,
        "the second word's identity set must start empty"
    );
}

#[test]
fn per_word_profile_does_not_accumulate_across_repeated_batch_words() {
    if std::env::var(PER_WORD_PROFILE_CHILD_ENV).as_deref() == Ok(PER_WORD_PROFILE_CHILD_VALUE) {
        assert_repeated_batch_word_profile_is_per_word();
        println!("{PER_WORD_PROFILE_CHILD_MARKER}");
        return;
    }

    let executable = std::env::current_exe().expect("resolve current test executable");
    let child = Command::new(executable)
        .args(["--exact", PER_WORD_PROFILE_TEST_NAME, "--nocapture"])
        .env("HC_ALT_YIELD", "1")
        .env(PER_WORD_PROFILE_CHILD_ENV, PER_WORD_PROFILE_CHILD_VALUE)
        .output()
        .expect("spawn isolated profiling regression process");
    let stdout = String::from_utf8_lossy(&child.stdout);
    let stderr = String::from_utf8_lossy(&child.stderr);
    assert!(
        child.status.success(),
        "isolated profiling regression failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains(PER_WORD_PROFILE_CHILD_MARKER),
        "child exited without executing its effect assertions\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn run_batch_sidecar_custom(
    tag: &str,
    grammar_xml: &str,
    words_text: &str,
    extra_args: &[&str],
) -> Vec<serde_json::Value> {
    let dir = scratch_dir(tag);
    let grammar_path = dir.join("grammar.xml");
    fs::write(&grammar_path, grammar_xml).expect("write grammar");
    run_batch_sidecar_path(tag, &grammar_path, words_text, extra_args)
}

fn run_batch_sidecar_path(
    tag: &str,
    grammar_path: &std::path::Path,
    words_text: &str,
    extra_args: &[&str],
) -> Vec<serde_json::Value> {
    let dir = scratch_dir(tag);
    let words_path = dir.join("words.txt");
    let out_path = dir.join("out.tsv");
    let analyses_path = dir.join("analyses.jsonl");
    fs::write(&words_path, words_text).expect("write words");

    let mut args: Vec<String> = vec![
        grammar_path.to_string_lossy().into_owned(),
        words_path.to_string_lossy().into_owned(),
        out_path.to_string_lossy().into_owned(),
        "--analyses".to_string(),
        analyses_path.to_string_lossy().into_owned(),
    ];
    args.extend(extra_args.iter().map(|s| s.to_string()));

    run_batch(&args).unwrap_or_else(|e| panic!("run_batch failed: {e}"));
    fs::read_to_string(&analyses_path)
        .expect("read analyses.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid sidecar JSON row"))
        .collect()
}

fn fwdata_fixture_with_k() -> std::path::PathBuf {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pg-fwdata/tests/data/fixture.fwdata");
    let dir = scratch_dir("analyses-fwdata-source-guids");
    let target = dir.join("fixture-with-k.fwdata");
    let xml = fs::read_to_string(source).expect("read fwdata fixture");
    // The element only, never its line break: the fixture is LF in the index, CRLF in a Windows tree.
    let dangling_env = "<objsur guid=\"00000000-0000-0000-0000-0000000000ff\" t=\"r\" />";
    assert!(
        xml.contains(dangling_env),
        "fixture dangling-environment shape changed"
    );
    let xml = xml.replacen(dangling_env, "", 1);
    let phoneme_ref = "<objsur guid=\"00000000-0000-0000-0000-00000000001b\" t=\"o\" />";
    let phoneme_ref_with_k =
        format!("{phoneme_ref}\n<objsur guid=\"00000000-0000-0000-0000-000000000025\" t=\"o\" />");
    assert!(
        xml.contains(phoneme_ref),
        "fixture phoneme set shape changed"
    );
    let xml = xml.replacen(phoneme_ref, &phoneme_ref_with_k, 1);
    let entry_marker = "<rt class=\"LexEntry\" guid=\"00000000-0000-0000-0000-000000000030\">";
    let k_records = r#"<rt class="PhPhoneme" guid="00000000-0000-0000-0000-000000000025" ownerguid="00000000-0000-0000-0000-00000000000f">
<Codes>
<objsur guid="00000000-0000-0000-0000-000000000026" t="o" />
</Codes>
<Name>
<AUni ws="fx">k</AUni>
</Name>
</rt>
<rt class="PhCode" guid="00000000-0000-0000-0000-000000000026" ownerguid="00000000-0000-0000-0000-000000000025">
<Representation>
<AUni ws="fx">k</AUni>
</Representation>
</rt>
"#;
    assert!(xml.contains(entry_marker), "fixture lexicon shape changed");
    let xml = xml.replacen(entry_marker, &format!("{k_records}{entry_marker}"), 1);
    fs::write(&target, xml).expect("write local fwdata fixture copy");
    target
}

#[test]
fn analyses_sidecar_keeps_closed_rows_and_duplicate_word_indexes() {
    for (tag, threads) in [("analyses-seq", "1"), ("analyses-par", "2")] {
        let rows =
            run_batch_sidecar_custom(tag, MINI_GRAMMAR_XML, "kat\nkat\n", &["--threads", threads]);
        assert_eq!(rows.len(), 2, "threads={threads}: {rows:?}");
        for (expected_index, row) in rows.iter().enumerate() {
            assert_eq!(row["schema"], "fieldworks-parse-analysis/v1");
            assert_eq!(row["index"], expected_index);
            assert_eq!(row["word"], "kat");
            assert!(row["elapsedMs"].is_u64());
            assert_eq!(row["capped"], false);
            assert_eq!(row["timedOut"], false);
            assert_eq!(row["invalidShape"], false);
            assert!(row["analyses"].is_array());
            assert!(row["unavailable"].is_array());
            assert!(
                row["analyses"].as_array().unwrap().len()
                    + row["unavailable"].as_array().unwrap().len()
                    > 0
            );
        }
    }
}

#[test]
fn analyses_sidecar_projects_source_guids_from_fwdata() {
    let fixture = fwdata_fixture_with_k();
    // The literal rewrite makes kad the surface; see retired divergence 071 (docs/divergences/README.md, Retired entries).
    let rows = run_batch_sidecar_path("analyses-fwdata-source-guids", &fixture, "kad\nkat\n", &[]);
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(row["capped"], false);
        assert_eq!(row["timedOut"], false);
        assert_eq!(row["invalidShape"], false);
        assert_eq!(row["unavailable"], serde_json::json!([]));
    }
    let row = &rows[0];
    assert_eq!(row["word"], "kad");
    assert_eq!(row["analyses"].as_array().unwrap().len(), 1);
    let morph = row["analyses"][0]["morphs"][0]
        .as_object()
        .expect("the fwdata fixture analysis should project");
    assert_eq!(morph["form"], "00000000-0000-0000-0000-000000000031");
    assert_eq!(morph["msa"], "00000000-0000-0000-0000-000000000032");
    assert_eq!(morph["inflType"], serde_json::Value::Null);
    assert_eq!(morph["guessedString"], serde_json::Value::Null);
    assert_eq!(rows[1]["word"], "kat");
    assert_eq!(rows[1]["analyses"], serde_json::json!([]));
}

#[test]
fn morph_spans_are_opt_in_and_place_each_morph_on_the_surface() {
    let fixture = fwdata_fixture_with_k();
    for (tag, threads) in [("spans-seq", "1"), ("spans-par", "2")] {
        let plain = run_batch_sidecar_path(tag, &fixture, "kad\n", &["--threads", threads]);
        assert!(plain[0]["analyses"][0].get("morphSpans").is_none());
        let rows = run_batch_sidecar_path(
            tag,
            &fixture,
            "kad\n",
            &["--threads", threads, "--morph-spans"],
        );
        let spans = &rows[0]["analyses"][0]["morphSpans"];
        assert_eq!(spans["surface"], "kad");
        assert_eq!(spans["segmentCount"], 3);
        assert_eq!(spans["morphemes"][0]["item"], 0);
        assert_eq!(spans["morphemes"][0]["reason"], serde_json::Value::Null);
        assert_eq!(
            spans["morphemes"][0]["spans"],
            serde_json::json!([{"segments": [0, 3], "offsets": [0, 3], "text": "kad"}])
        );
        assert_eq!(spans["unattributed"], serde_json::json!([]));
        assert_eq!(
            rows[0]["analyses"][0]["morphs"],
            plain[0]["analyses"][0]["morphs"]
        );
    }
}

#[test]
fn morph_spans_are_refused_where_they_cannot_run() {
    let fixture = fwdata_fixture_with_k();
    let dir = scratch_dir("spans-refused");
    let words = dir.join("words.txt");
    fs::write(&words, "kat\n").unwrap();
    let analyses = dir.join("a.jsonl");
    let base = |extra: &[&str]| -> Vec<String> {
        let mut args: Vec<String> = vec![
            fixture.to_string_lossy().into_owned(),
            words.to_string_lossy().into_owned(),
            dir.join("out.tsv").to_string_lossy().into_owned(),
            "--morph-spans".to_string(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        args
    };
    let analyses_path = analyses.to_string_lossy().into_owned();
    let with_analyses = ["--analyses", analyses_path.as_str()];
    let cases: Vec<Vec<&str>> = vec![
        vec![],
        [&with_analyses[..], &["--stats"]].concat(),
        [&with_analyses[..], &["--partials", "1"]].concat(),
        [&with_analyses[..], &["--trace-rejections"]].concat(),
        [&with_analyses[..], &["--prefilter", "on"]].concat(),
        [&with_analyses[..], &["--engine", "fst"]].concat(),
    ];
    for extra in cases {
        let error = run_batch(&base(&extra)).expect_err(&format!("{extra:?} is refused"));
        assert!(!error.is_empty(), "{extra:?}");
        // Only the no-sidecar case is the morph-spans check's own message; the others are refused by their flags' checks.
        if extra.is_empty() {
            assert!(
                error.contains("--morph-spans requires --analyses"),
                "{error}"
            );
        }
    }
}

/// The raw bytes of a flag-less row, as v0.33 wrote them: the morph keys keep their declared order.
#[test]
fn analyses_rows_without_the_flag_keep_their_exact_bytes() {
    let fixture = fwdata_fixture_with_k();
    let dir = scratch_dir("analyses-bytes");
    let words = dir.join("words.txt");
    let analyses = dir.join("a.jsonl");
    fs::write(&words, "kad\n").unwrap();
    let args: Vec<String> = vec![
        fixture.to_string_lossy().into_owned(),
        words.to_string_lossy().into_owned(),
        dir.join("out.tsv").to_string_lossy().into_owned(),
        "--analyses".to_string(),
        analyses.to_string_lossy().into_owned(),
    ];
    run_batch(&args).unwrap();
    let line = fs::read_to_string(&analyses).unwrap();
    let (head, tail) = line.split_once("\"elapsedMs\":").expect("elapsedMs");
    let tail = tail.split_once(',').expect("elapsedMs value").1;
    assert_eq!(
        head,
        "{\"schema\":\"fieldworks-parse-analysis/v1\",\"index\":0,\"word\":\"kad\","
    );
    assert_eq!(
        tail,
        "\"capped\":false,\"timedOut\":false,\"invalidShape\":false,\"analyses\":[{\"morphs\":[{\"form\":\"00000000-0000-0000-0000-000000000031\",\"msa\":\"00000000-0000-0000-0000-000000000032\",\"inflType\":null,\"guessedString\":null}]}],\"unavailable\":[]}\n"
    );
}

/// A FieldWorks circumfix reaches the CLI with each half placed, and the output accounts for the surface.
#[test]
fn morph_spans_place_each_circumfix_half_through_the_cli() {
    let fixture = pg_conformance_fixtures::require_fixture(
        "edge-cases",
        "adhoc-prohibition-circumfix-pairings",
    );
    let project = fixture.dir.join("fieldworks").join("project.fwdata");
    let rows = run_batch_sidecar_path(
        "spans-circumfix",
        &project,
        "tagaka\ntogaki\n",
        &["--morph-spans"],
    );
    let mut readings = 0;
    for row in &rows {
        for reading in row["analyses"].as_array().unwrap() {
            let spans = &reading["morphSpans"];
            let surface = spans["surface"].as_str().unwrap();
            let mut runs: Vec<(u64, u64, String)> = Vec::new();
            let all = spans["morphemes"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|m| {
                    assert_eq!(m["spans"].is_null(), !m["reason"].is_null(), "{m}");
                    m["spans"].as_array().cloned().unwrap_or_default()
                })
                .chain(spans["unattributed"].as_array().unwrap().iter().cloned());
            for run in all {
                runs.push((
                    run["segments"][0].as_u64().unwrap(),
                    run["segments"][1].as_u64().unwrap(),
                    run["text"].as_str().unwrap().to_string(),
                ));
            }
            runs.sort();
            let mut next = 0;
            for (start, end, _) in &runs {
                assert_eq!(*start, next, "{spans}");
                next = *end;
            }
            assert_eq!(next, spans["segmentCount"].as_u64().unwrap());
            let text: String = runs.iter().map(|(_, _, t)| t.as_str()).collect();
            assert_eq!(text, surface);
            let word = row["word"].as_str().unwrap();
            let halves: Vec<&str> = spans["morphemes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["spans"][0]["text"].as_str().unwrap())
                .collect();
            assert_eq!(halves, [&word[..2], "ga", &word[4..]]);
            readings += 1;
        }
    }
    assert!(readings >= 2, "both words parse");
}

#[test]
fn analyses_sidecar_preserves_cap_and_timeout_flags() {
    let grammar_xml = homophonous_suffix_grammar_xml(7);
    let capped_rows = run_batch_sidecar_custom(
        "analyses-cap-timeout",
        &grammar_xml,
        &format!("kad{}\n", "d".repeat(7)),
        &["--work-cap", "50000"],
    );
    assert_eq!(capped_rows.len(), 1);
    assert_eq!(capped_rows[0]["capped"], true);
    assert_eq!(capped_rows[0]["timedOut"], false);
    assert!(capped_rows[0]["analyses"].is_array());
    assert!(capped_rows[0]["unavailable"].is_array());

    let timed_out_rows = run_batch_sidecar_custom(
        "analyses-timeout",
        MINI_GRAMMAR_XML,
        "kat\n",
        &["--word-timeout-ms", "0"],
    );
    assert_eq!(timed_out_rows.len(), 1);
    assert_eq!(timed_out_rows[0]["capped"], false);
    assert_eq!(timed_out_rows[0]["timedOut"], true);

    let grammar = pg_grammar::load(MINI_GRAMMAR_XML).expect("load mini grammar");
    let outcome = pg_parse::ParseOutcome {
        partials: Vec::new(),
        analyses: vec![("partial".into(), "kat".into())],
        structured: Vec::new(),
        capped: true,
        capture_capped: false,
        word_mem_capped: false,
        invalid_shape: false,
        work_steps: 0,
        timed_out: true,
        guessed: false,
        candidates_generated: 0,
        cost: None,
    };
    let mut encoded = Vec::new();
    write_parse_analysis_row(&mut encoded, 0, "kat", 12, &outcome, &grammar, None)
        .expect("serialize mixed-bound outcome");
    let row: serde_json::Value = serde_json::from_slice(&encoded).expect("valid row");
    assert!(row.get("completion").is_none());
    assert_eq!(row["capped"], true);
    assert_eq!(row["timedOut"], true);
}

#[test]
fn analyses_sidecar_rejects_resume_and_path_collisions() {
    let dir = scratch_dir("analyses-paths");
    let grammar_path = dir.join("grammar.xml");
    let words_path = dir.join("words.txt");
    let out_path = dir.join("out.tsv");
    fs::write(&grammar_path, MINI_GRAMMAR_XML).expect("write grammar");
    fs::write(&words_path, "kat\n").expect("write words");

    let err = run_batch(&[
        grammar_path.to_string_lossy().into_owned(),
        words_path.to_string_lossy().into_owned(),
        out_path.to_string_lossy().into_owned(),
        "--start".to_string(),
        "1".to_string(),
        "--analyses".to_string(),
        dir.join("analyses.jsonl").to_string_lossy().into_owned(),
    ])
    .expect_err("sidecar cannot resume from a nonzero start index");
    assert!(err.contains("--start"), "{err}");

    let err = run_batch(&[
        grammar_path.to_string_lossy().into_owned(),
        words_path.to_string_lossy().into_owned(),
        out_path.to_string_lossy().into_owned(),
        "--analyses".to_string(),
        words_path.to_string_lossy().into_owned(),
    ])
    .expect_err("sidecar cannot overwrite the word list");
    assert!(err.contains("overwrite"), "{err}");

    let analyses_path = dir.join("analyses-cache-collision.jsonl");
    let err = run_batch(&[
        grammar_path.to_string_lossy().into_owned(),
        words_path.to_string_lossy().into_owned(),
        out_path.to_string_lossy().into_owned(),
        "--analyses".to_string(),
        analyses_path.to_string_lossy().into_owned(),
        "--stats".to_string(),
        "--cache".to_string(),
        analyses_path.to_string_lossy().into_owned(),
    ])
    .expect_err("sidecar cannot overwrite the stats cache");
    assert!(err.contains("stats cache"), "{err}");

    let default_cache =
        pg_stats::default_cache_path(&grammar_path).expect("resolve default stats cache");
    let default_cache_text = default_cache.to_string_lossy().into_owned();
    let cache_before = fs::read(&default_cache).ok();
    let default_out = dir.join("default-cache-out.tsv");
    let err = run_batch(&[
        grammar_path.to_string_lossy().into_owned(),
        words_path.to_string_lossy().into_owned(),
        default_out.to_string_lossy().into_owned(),
        "--analyses".to_string(),
        default_cache_text,
        "--stats".to_string(),
    ])
    .expect_err("sidecar cannot overwrite the default stats cache");
    assert!(err.contains("stats cache"), "{err}");
    assert!(!default_out.exists(), "TSV output must not be created");
    match cache_before {
        Some(contents) => assert_eq!(fs::read(&default_cache).unwrap(), contents),
        None => assert!(!default_cache.exists(), "cache must not be created"),
    }
}

#[test]
fn analyses_sidecar_invalid_shape_has_empty_projection_arrays() {
    let rows = run_batch_sidecar_custom("analyses-invalid-shape", MINI_GRAMMAR_XML, "zzz\n", &[]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["invalidShape"], true);
    assert_eq!(rows[0]["analyses"].as_array().unwrap().len(), 0);
    assert_eq!(rows[0]["unavailable"].as_array().unwrap().len(), 0);
}

/// Sequential path (`--threads 1`): a `--word-timeout-ms=0` deadline must produce a `STARTED` sentinel followed by a `TIMEOUT`/`-` result row, matching the shape an external watchdog synthesizes for a killed stall.
#[test]
fn word_timeout_ms_zero_writes_timeout_row_single_threaded() {
    let lines = run_batch_tsv("seq-timeout", &["--word-timeout-ms", "0", "--threads", "1"]);
    assert_eq!(lines.len(), 2, "STARTED sentinel + result row: {lines:?}");
    assert_eq!(lines[0], "0\tkat\tSTARTED");
    let fields: Vec<&str> = lines[1].split('\t').collect();
    assert_eq!(fields.len(), 5, "idx/word/ms/status/signature: {fields:?}");
    assert_eq!(fields[0], "0");
    assert_eq!(fields[1], "kat");
    fields[2]
        .parse::<u128>()
        .expect("ms column must be an integer");
    assert_eq!(fields[3], "TIMEOUT");
    assert_eq!(fields[4], "-");
}

/// Parallel path (`--threads 2`): same deadline, same row shape, but no `STARTED` line at all.
#[test]
fn word_timeout_ms_zero_writes_timeout_row_parallel() {
    let lines = run_batch_tsv("par-timeout", &["--word-timeout-ms", "0", "--threads", "2"]);
    assert_eq!(
        lines.len(),
        1,
        "no STARTED line in the parallel writer: {lines:?}"
    );
    let fields: Vec<&str> = lines[0].split('\t').collect();
    assert_eq!(fields.len(), 5);
    assert_eq!(fields[0], "0");
    assert_eq!(fields[1], "kat");
    fields[2]
        .parse::<u128>()
        .expect("ms column must be an integer");
    assert_eq!(fields[3], "TIMEOUT");
    assert_eq!(fields[4], "-");
}

/// Control: omitting `--word-timeout-ms` must keep producing the pre-existing `ok` row shape, in both thread modes.
#[test]
fn no_word_timeout_flag_keeps_ok_row_shape_both_thread_modes() {
    for (tag, threads) in [("seq-ok", "1"), ("par-ok", "2")] {
        let lines = run_batch_tsv(tag, &["--threads", threads]);
        let result_line = lines.last().expect("at least one line");
        let fields: Vec<&str> = result_line.split('\t').collect();
        assert_eq!(fields.len(), 5, "threads={threads}: {fields:?}");
        assert_eq!(fields[3], "ok", "threads={threads}: {fields:?}");
        assert_ne!(
            fields[4], "-",
            "threads={threads}: \"kat\" should analyze to a real signature"
        );
    }
}

/// `--work-columns` appends the word's own `work_steps`, identical in both writer paths and after `guessed`; without it the row is unchanged.
#[test]
fn work_columns_append_the_words_work_steps() {
    let grammar_xml = homophonous_suffix_grammar_xml(2);
    let dir = scratch_dir("row-counts-grammar");
    let grammar_path = dir.join("grammar.xml");
    fs::write(&grammar_path, &grammar_xml).unwrap();
    let (grammar, _) = load_grammar(&grammar_path.to_string_lossy()).unwrap();
    let expected =
        pg_parse::Morpher::new(&grammar, DEFAULT_WORK_CAP.as_morpher_cap()).parse_word("kadd");
    assert!(expected.work_steps > 0);
    let counts = [expected.work_steps.to_string()];
    for (tag, args) in [
        ("row-counts-seq", vec!["--threads", "1"]),
        ("row-counts-par", vec!["--threads", "2"]),
        ("row-counts-guess", vec!["--threads", "1", "--guess"]),
    ] {
        let guessed = args.contains(&"--guess");
        let plain = run_batch_tsv_custom(tag, &grammar_xml, "kadd\n", &args);
        let plain: Vec<&str> = plain.last().unwrap().split('\t').collect();
        assert_eq!(plain.len(), if guessed { 6 } else { 5 }, "{tag}: {plain:?}");
        let mut with_counts = args.clone();
        with_counts.push("--work-columns");
        let lines = run_batch_tsv_custom(tag, &grammar_xml, "kadd\n", &with_counts);
        let fields: Vec<&str> = lines.last().unwrap().split('\t').collect();
        assert_eq!(fields[3], "ok", "{tag}: {fields:?}");
        assert_eq!(fields.len(), plain.len() + 1, "{tag}: {fields:?}");
        assert_eq!(fields[..2], plain[..2], "{tag}");
        assert_eq!(fields[3..plain.len()], plain[3..], "{tag}");
        assert_eq!(fields[fields.len() - 1..], counts, "{tag}: {fields:?}");
    }
}

/// A work-cap stop is deterministic: with `--work-columns` the row records exactly the configured allowance.
#[test]
fn a_cap_row_reports_the_work_spent_up_to_the_cap() {
    for threads in ["1", "2"] {
        let rows = run_batch_tsv(
            "cap-row-counts",
            &["--work-cap", "1", "--threads", threads, "--work-columns"],
        );
        let fields: Vec<_> = rows.last().unwrap().split('\t').collect();
        assert_eq!(fields[3], "CAP", "{fields:?}");
        assert_eq!(fields[5..], ["1"], "{fields:?}");
    }
}
/// `k` homophonous one-shot suffix rules unapplying "d", so the unwind is genuinely combinatorial; `MINI_GRAMMAR_XML`'s root-only lookup takes zero steps and cannot exercise a real `--work-cap` firing.
fn homophonous_suffix_grammar_xml(k: usize) -> String {
    let mut mrule_defs = String::new();
    let mut ids = Vec::with_capacity(k);
    for i in 0..k {
        mrule_defs.push_str(&format!(
            r#"<MorphologicalRule id="mrD{i}" requiredPartsOfSpeech="posV"><Name>d_suffix_{i}</Name><MorphemeId>D{i}</MorphemeId>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="subD{i}">
                  <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                  <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
            </MorphologicalRule>"#
        ));
        ids.push(format!("mrD{i}"));
    }
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>WorkCapCliTest</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>Verb</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="{ids}">
        <Name>main</Name>
        <MorphologicalRuleDefinitions>{mrule_defs}</MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eroot" partOfSpeech="posV">
            <MorphemeId>ROOT</MorphemeId>
            <Allomorphs><Allomorph id="aroot"><PhoneticShape>kad</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#,
        ids = ids.join(" "),
    )
}

/// A small `--work-cap` fires on a genuinely combinatorial unwind, in both thread modes; an incomplete outcome is typed `CAP`, never an `ok` row.
#[test]
fn work_cap_small_writes_cap_row_both_thread_modes() {
    let grammar_xml = homophonous_suffix_grammar_xml(7);
    let word = format!("kad{}\n", "d".repeat(7));
    for (tag, threads) in [("seq-cap", "1"), ("par-cap", "2")] {
        let lines = run_batch_tsv_custom(
            tag,
            &grammar_xml,
            &word,
            &["--work-cap", "50000", "--threads", threads],
        );
        let result_line = lines.last().expect("at least one line");
        let fields: Vec<&str> = result_line.split('\t').collect();
        assert_eq!(fields.len(), 5, "threads={threads}: {fields:?}");
        assert_eq!(fields[3], "CAP", "threads={threads}: {fields:?}");
        assert!(
            !fields[4].is_empty(),
            "threads={threads}: partial signature column must not be empty: {fields:?}"
        );
    }
}

/// `--step-cap` counted analysis attempts and no longer exists: every subcommand that took it refuses it as an unknown option.
#[test]
fn the_removed_step_cap_flag_is_refused_like_any_unknown_option() {
    let to_args = |args: &[&str]| -> Vec<String> { args.iter().map(|a| a.to_string()).collect() };
    let refusals = [
        run_batch(&to_args(&["g.xml", "w.txt", "o.tsv", "--step-cap", "5"])),
        run_batch(&to_args(&["g.xml", "w.txt", "o.tsv", "--step-cap=5"])),
        super::run_parse(&to_args(&["g.xml", "kat", "--step-cap", "5"])),
        super::run_parse(&to_args(&["g.xml", "kat", "--step-cap=5"])),
        super::stats_cmd::run_stats(&to_args(&["g.xml", "--step-cap", "5"]))
            .map_err(|e| e.to_string()),
        super::serve::parse_args(&to_args(&["--project", "p.fwdata", "--step-cap", "5"]))
            .map(|_| ()),
    ];
    for refusal in refusals {
        let err = refusal.expect_err("--step-cap must be refused");
        assert!(err.contains("unknown option: --step-cap"), "{err}");
    }
}

#[test]
fn capture_cap_is_typed_in_both_batch_writer_paths_and_can_be_disabled() {
    let grammar = homophonous_suffix_grammar_xml(1);
    for threads in ["1", "2"] {
        let capped = run_batch_tsv_custom(
            "capture-cap",
            &grammar,
            "kadd\n",
            &["--capture-cap-mib", "0", "--threads", threads],
        );
        let fields: Vec<_> = capped.last().unwrap().split('\t').collect();
        assert_eq!(fields[3], "MEMCAP", "{fields:?}");
        let complete = run_batch_tsv_custom(
            "capture-unbounded",
            &grammar,
            "kadd\n",
            &["--capture-cap-mib=unbounded", "--threads", threads],
        );
        let fields: Vec<_> = complete.last().unwrap().split('\t').collect();
        assert_eq!(fields[3], "ok", "{fields:?}");
        assert_ne!(
            fields[4], "-",
            "the word must parse when the cap is disabled"
        );
    }
}

#[test]
fn word_only_reuse_matches_fresh_cache_after_search_settings_change() {
    let dir = tempfile::tempdir().unwrap();
    let grammar = dir.path().join("grammar.xml");
    let words = dir.path().join("words.txt");
    let out = dir.path().join("out.tsv");
    let reused = dir.path().join("reused.sqlite");
    let fresh = dir.path().join("fresh.sqlite");
    fs::write(&grammar, homophonous_suffix_grammar_xml(1)).unwrap();
    fs::write(&words, "kadd\n").unwrap();

    let batch = |cache: &std::path::Path, capture, partials| {
        let mut args = vec![
            grammar.to_str().unwrap(),
            words.to_str().unwrap(),
            out.to_str().unwrap(),
            "--threads",
            "1",
            "--work-columns",
            "--stats",
            "--stats-profile=word-only",
            "--cache",
            cache.to_str().unwrap(),
            capture,
        ];
        if let Some(partials) = partials {
            args.extend([partials, "--prefilter=off"]);
        }
        run_batch(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).unwrap();
        fs::read_to_string(&out).unwrap()
    };
    let cached_word = |cache| {
        rusqlite::Connection::open(cache)
            .unwrap()
            .query_row("SELECT capped, passes, work, run_id FROM word", [], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .unwrap()
    };

    let tsv = batch(&reused, "--capture-cap-mib=0", None);
    assert_eq!(
        tsv.lines().last().unwrap().split('\t').nth(3),
        Some("MEMCAP")
    );
    let capped = cached_word(&reused);
    let tsv = batch(&reused, "--capture-cap-mib=unbounded", None);
    let fields: Vec<_> = tsv.lines().last().unwrap().split('\t').collect();
    assert_eq!(fields[3], "ok");
    batch(&fresh, "--capture-cap-mib=unbounded", None);
    let fresh_word = cached_word(&fresh);
    let updated = cached_word(&reused);
    assert_eq!(
        (updated.0, updated.1, updated.2),
        (fresh_word.0, fresh_word.1, fresh_word.2)
    );
    assert_eq!((updated.0, updated.2), (0, fields[5].parse().unwrap()));
    assert_ne!(updated.3, capped.3);

    batch(&reused, "--capture-cap-mib=unbounded", Some("--partials=1"));
    let partial = cached_word(&reused);
    assert_ne!(
        partial.3, updated.3,
        "the partial limit changes the owning measurement"
    );
    batch(&reused, "--capture-cap-mib=unbounded", Some("--partials=1"));
    assert_eq!(cached_word(&reused), partial);
}

#[test]
fn word_memory_cap_is_typed_and_the_batch_continues_in_both_thread_modes() {
    let grammar = homophonous_suffix_grammar_xml(1);
    let mut expected = std::collections::BTreeMap::new();
    for threads in ["1", "2"] {
        for words in ["kadd\nka\nkadd\nka\n", "ka\nkadd\nka\nkadd\n"] {
            let rows = run_batch_tsv_custom(
                "word-memory-cap",
                &grammar,
                words,
                &[
                    "--word-mem-cap-mib",
                    "1",
                    "--threads",
                    threads,
                    "--work-columns",
                ],
            );
            let completed: Vec<Vec<&str>> = rows
                .iter()
                .map(|line| line.split('\t').collect::<Vec<_>>())
                .filter(|row| row.len() == 6)
                .collect();
            assert_eq!(completed.len(), 4);
            for row in completed {
                assert_eq!(row[3], if row[1] == "kadd" { "MEMCAP" } else { "ok" });
                let normalized: Vec<String> = [row[1], row[3], row[4], row[5]]
                    .into_iter()
                    .map(str::to_owned)
                    .collect();
                if let Some(previous) = expected.get(row[1]) {
                    assert_eq!(&normalized, previous, "threads={threads}, words={words:?}");
                } else {
                    expected.insert(row[1].to_owned(), normalized);
                }
            }
        }
        let complete = run_batch_tsv_custom(
            "word-memory-unbounded",
            &grammar,
            "kadd\n",
            &["--word-mem-cap-mib=0", "--threads", threads],
        );
        assert_eq!(complete.last().unwrap().split('\t').nth(3), Some("ok"));
    }
}

#[test]
fn word_memory_cap_cli_values_are_checked() {
    assert_eq!(super::parse_word_mem_cap("0"), Ok(None));
    assert_eq!(super::parse_word_mem_cap("000"), Ok(None));
    assert_eq!(
        super::parse_word_mem_cap("100"),
        Ok(Some(pg_parse::morpher::DEFAULT_WORD_MEM_CAP_BYTES))
    );
    for value in ["-1", "abc", &usize::MAX.to_string()] {
        assert!(super::parse_word_mem_cap(value).is_err());
    }
}

#[test]
fn capture_cap_cli_values_are_checked_before_file_access() {
    assert_eq!(super::parse_capture_cap("unbounded"), Ok(None));
    assert_eq!(
        super::parse_capture_cap("256"),
        Ok(Some(pg_fst::work::DEFAULT_RETAINED_CAP_BYTES))
    );
    for value in ["-1", "abc", &usize::MAX.to_string()] {
        assert!(super::parse_capture_cap(value).is_err());
    }
}

#[test]
fn default_work_cap_is_five_billion() {
    assert_eq!(
        DEFAULT_WORK_CAP,
        WorkCap::Finite(std::num::NonZeroU64::new(5_000_000_000).unwrap())
    );
}

/// `--work-cap 0` is rejected before any file is touched, with the same message `WorkCap::from_str` gives.
#[test]
fn work_cap_zero_is_rejected_with_a_specific_message() {
    let err = run_batch(&[
        "unused.xml".to_string(),
        "unused.txt".to_string(),
        "unused.tsv".to_string(),
        "--work-cap".to_string(),
        "0".to_string(),
    ])
    .expect_err("--work-cap 0 must be rejected");
    assert!(err.contains("fires before the first work step"), "{err}");
}

/// `--work-cap unbounded` opts back into no bound at all; "kat" analyzes in far fewer than `DEFAULT_WORK_CAP` work steps regardless, so this only proves the flag parses and the run still completes normally.
#[test]
fn work_cap_unbounded_parses_and_batch_completes_ok() {
    let lines = run_batch_tsv("unbounded-ok", &["--work-cap", "unbounded"]);
    let result_line = lines.last().expect("at least one line");
    let fields: Vec<&str> = result_line.split('\t').collect();
    assert_eq!(fields[3], "ok", "{fields:?}");
}

#[test]
fn batch_stats_identities_match_full_import_and_legacy_fingerprints() {
    use super::{stats_manifest, LoadedStatsIdentity, StatsIdentityRequest};

    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pg-fwdata/tests/data/fixture.fwdata");
    let xml = fs::read_to_string(&fixture).expect("read synthetic fixture");
    let xml = xml.replace(
        "guid=\"00000000-0000-0000-0000-0000000000ff\"",
        "guid=\"00000000-0000-0000-0000-000000000017\"",
    );
    let scratch = tempfile::tempdir().expect("stats identity scratch");
    let path = scratch.path().join("fixture.fwdata");
    fs::write(&path, &xml).expect("write adjusted synthetic fixture");

    let loaded =
        super::load_grammar_impl(&path.to_string_lossy(), false, StatsIdentityRequest::Frozen)
            .unwrap_or_else(|e| panic!("fixture loads: {e}"));
    let LoadedStatsIdentity::Frozen(identity) = loaded.stats_identity else {
        panic!("frozen stats identity must be captured");
    };
    let (snapshot, _) = pg_fwdata::import_file(&path).expect("full import");
    let expected =
        stats_manifest::SourceIdentity::from_imported_snapshot(xml.as_bytes(), &snapshot)
            .expect("full identity");
    assert_eq!(identity.model_fingerprint, expected.model_fingerprint);
    assert_eq!(identity.grammar_hash, expected.grammar_hash);

    let words = scratch.path().join("words.txt");
    fs::write(&words, "kat\n").unwrap();
    let tsv = scratch.path().join("out.tsv");
    let cases = [
        ("fixture.fwdata", xml.as_str(), snapshot.grammar_hash()),
        (
            "raw.FWDATA",
            xml.as_str(),
            stats_manifest::sha256_hex(xml.as_bytes()),
        ),
        ("fixture.json", &snapshot.to_json(), snapshot.grammar_hash()),
        (
            "fixture.xml",
            MINI_GRAMMAR_XML,
            stats_manifest::sha256_hex(MINI_GRAMMAR_XML.as_bytes()),
        ),
    ];
    for (name, source, expected_hash) in cases {
        let grammar = scratch.path().join(name);
        fs::write(&grammar, source).unwrap();
        let cache = scratch.path().join(format!("{name}.sqlite"));
        let args = [
            grammar.to_str().unwrap(),
            words.to_str().unwrap(),
            tsv.to_str().unwrap(),
            "--threads",
            "1",
            "--stats",
            "--cache",
            cache.to_str().unwrap(),
        ]
        .map(str::to_string);
        run_batch(&args).unwrap_or_else(|error| panic!("{name} failed: {error}"));
        let db = rusqlite::Connection::open(&cache).unwrap();
        let captured_hash: String = db
            .query_row("SELECT grammar_hash FROM cache_identity", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(captured_hash, expected_hash, "{name}");
    }
}

#[test]
fn fwdata_load_keeps_structured_warnings_and_collapses_import_compile_duplicates() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pg-fwdata/tests/data/fixture.fwdata");
    let xml = fs::read_to_string(&fixture).expect("read synthetic fixture");
    let xml = xml.replace(
        "guid=\"00000000-0000-0000-0000-0000000000ff\"",
        "guid=\"00000000-0000-0000-0000-000000000017\"",
    );
    let fixture_copy = scratch_dir("structured-fwdata-warnings").join("fixture.fwdata");
    fs::write(&fixture_copy, xml).expect("write adjusted synthetic fixture");
    let (_, warnings) = load_grammar(&fixture_copy.to_string_lossy()).expect("fixture loads");
    let unknown_morph_type: Vec<_> = warnings
        .iter()
        .filter(|warning| warning.code == "fwdata.unknown-morph-type-guid")
        .collect();

    assert_eq!(unknown_morph_type.len(), 1, "{unknown_morph_type:#?}");
    assert_eq!(
        unknown_morph_type[0].message,
        "Allomorph 'xxx' has an unknown morph type and was skipped."
    );
    assert_eq!(
        unknown_morph_type[0].subjects[0].name.as_deref(),
        Some("xxx")
    );
    assert_eq!(
        unknown_morph_type[0].subjects[0].guid.as_deref(),
        Some("00000000-0000-0000-0000-000000000044")
    );
}

/// The synthetic `.fwdata` fixture with `extra` spliced into its escaped `<ParserParameters><HC>` string.
fn fwdata_fixture_with_hc_settings(tag: &str, extra: &str) -> std::path::PathBuf {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pg-fwdata/tests/data/fixture.fwdata");
    // The same dangling-environment repair `fwdata_load_keeps_structured_warnings_and_collapses_import_compile_duplicates` makes, so the fixture compiles.
    let xml = fs::read_to_string(&fixture)
        .expect("read synthetic fixture")
        .replace(
            "guid=\"00000000-0000-0000-0000-0000000000ff\"",
            "guid=\"00000000-0000-0000-0000-000000000017\"",
        );
    let anchor = "&lt;HC&gt;&lt;NotOnClitics&gt;";
    assert!(xml.contains(anchor), "the fixture's <HC> block moved");
    let xml = xml.replace(anchor, &format!("&lt;HC&gt;{extra}&lt;NotOnClitics&gt;"));
    let path = scratch_dir(tag).join("fixture.fwdata");
    fs::write(&path, xml).expect("write adjusted synthetic fixture");
    path
}

#[test]
fn project_make_partials_final_reaches_batch_and_an_explicit_flag_overrides_it() {
    let project = fwdata_fixture_with_hc_settings(
        "make-partials-final",
        "&lt;MakePartialsFinal&gt;true&lt;/MakePartialsFinal&gt;",
    );
    let project_arg = project.to_string_lossy().into_owned();
    let (grammar, _) = load_grammar(&project_arg).expect("fixture loads");
    assert!(grammar.always_enforce_final_templates);

    let dir = project.parent().unwrap().to_path_buf();
    let words = dir.join("words.txt");
    fs::write(&words, "kat\n").unwrap();
    let cache = dir.join("cache.sqlite3");
    let run = |extra: &[&str]| {
        let mut args = vec![
            project_arg.clone(),
            words.to_string_lossy().into_owned(),
            dir.join("out.tsv").to_string_lossy().into_owned(),
            "--threads".into(),
            "1".into(),
            "--stats".into(),
            "--cache".into(),
            cache.to_string_lossy().into_owned(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        run_batch(&args)
    };
    run(&[]).expect("the project's own value records into a fresh cache");
    run(&["--always-enforce-final-templates"]).expect("an explicit true agrees with the project");
    let err = run(&["--always-enforce-final-templates=false"])
        .expect_err("an explicit false must override the project's true");
    assert!(
        err.contains("records always_enforce_final_templates=true but requested false"),
        "{err}"
    );
    let err = run(&["--always-enforce-final-templates=maybe"]).unwrap_err();
    assert!(
        err.contains("--always-enforce-final-templates=maybe"),
        "{err}"
    );
}

/// The `<HC>`-adjusted fixture whose `ranna` entry becomes a noun with the lexical pattern `t[V]*d`, so a word such as `taad` has no lexicon root and only a guess.
fn fwdata_fixture_with_pattern_root(tag: &str, hc_extra: &str) -> std::path::PathBuf {
    let path = fwdata_fixture_with_hc_settings(tag, hc_extra);
    let xml = fs::read_to_string(&path).unwrap();
    let form = "<Form>\n<AUni ws=\"fx\">ranna</AUni>";
    let msa_pos = "guid=\"00000000-0000-0000-0000-000000000042\" ownerguid=\"00000000-0000-0000-0000-000000000040\">\n<PartOfSpeech>\n<objsur guid=\"00000000-0000-0000-0000-00000000000c\"";
    assert!(
        xml.contains(form) && xml.contains(msa_pos),
        "the fixture's ranna entry moved"
    );
    let xml = xml
        .replace(form, "<Form>\n<AUni ws=\"fx\">t[V]*d</AUni>")
        .replace(msa_pos, &msa_pos.replace("00000000000c", "00000000000b"));
    fs::write(&path, xml).unwrap();
    path
}

/// Runs `batch` over `words` with `extra` flags and returns the result rows (no `STARTED` lines) and the `--analyses` JSONL rows.
fn run_pattern_batch(
    project: &std::path::Path,
    words: &str,
    extra: &[&str],
) -> (Vec<Vec<String>>, Vec<serde_json::Value>) {
    let dir = project.parent().unwrap();
    let words_path = dir.join("words.txt");
    fs::write(&words_path, words).unwrap();
    let out = dir.join("out.tsv");
    let analyses = dir.join("analyses.jsonl");
    let mut args = vec![
        project.to_string_lossy().into_owned(),
        words_path.to_string_lossy().into_owned(),
        out.to_string_lossy().into_owned(),
        "--analyses".into(),
        analyses.to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    run_batch(&args).expect("batch runs");
    let rows = fs::read_to_string(&out)
        .unwrap()
        .lines()
        .filter(|line| !line.ends_with("\tSTARTED"))
        .map(|line| line.split('\t').map(str::to_string).collect())
        .collect();
    let analyses = fs::read_to_string(&analyses)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (rows, analyses)
}

#[test]
fn a_project_guesses_roots_by_default_without_changing_the_batch_tsv_layout() {
    let project = fwdata_fixture_with_pattern_root("guess-roots-default", "");
    let (grammar, _) = load_grammar(&project.to_string_lossy()).expect("fixture loads");
    assert!(
        grammar.guess_roots,
        "an absent <GuessRoots> means FieldWorks' default true"
    );

    for threads in ["1", "2"] {
        let (rows, analyses) =
            run_pattern_batch(&project, "kad\ntaad\ndat\n", &["--threads", threads]);
        assert_eq!(rows.len(), 3, "threads={threads}: {rows:?}");
        for row in &rows {
            assert_eq!(
                row.len(),
                5,
                "threads={threads}: the project setting must not add a column: {row:?}"
            );
        }
        assert!(
            rows[0][4] != "-",
            "threads={threads}: kad is kat in the lexicon, voiced after a vowel: {:?}",
            rows[0]
        );
        assert!(
            rows[1][4]
                .split(';')
                .all(|analysis| analysis == "taad|taad"),
            "threads={threads}: taad is found only by guessing: {:?}",
            rows[1]
        );
        assert_eq!(rows[2][4], "-", "threads={threads}: no pattern matches dat");

        assert_eq!(
            analyses[1]["unavailable"],
            serde_json::json!([]),
            "{:?}",
            analyses[1]
        );
        let guessed = analyses[1]["analyses"].as_array().unwrap();
        assert!(!guessed.is_empty());
        for analysis in guessed {
            assert_eq!(
            analysis["morphs"],
            serde_json::json!([{
                "form": "00000000-0000-0000-0000-000000000041",
                "msa": "00000000-0000-0000-0000-000000000042",
                "inflType": null,
                "guessedString": "taad",
            }]),
            "threads={threads}: FieldWorks reports the pattern's form and MSA plus the guessed text"
        );
        }
        assert_eq!(
            analyses[0]["analyses"][0]["morphs"][0]["guessedString"],
            serde_json::Value::Null
        );
    }
}

#[test]
fn an_explicit_guess_flag_overrides_the_project_and_alone_adds_the_guessed_column() {
    let project = fwdata_fixture_with_pattern_root("guess-roots-override", "");
    let (rows, _) = run_pattern_batch(&project, "kad\ntaad\ndat\n", &["--guess=false"]);
    assert!(rows.iter().all(|row| row.len() == 5), "{rows:?}");
    assert_eq!(
        rows[1][4], "-",
        "--guess=false turns the project's guessing off"
    );

    for flag in ["--guess", "--guess=true"] {
        let (rows, _) = run_pattern_batch(&project, "kad\ntaad\ndat\n", &[flag]);
        let guessed: Vec<&str> = rows.iter().map(|row| row[5].as_str()).collect();
        assert!(rows.iter().all(|row| row.len() == 6), "{flag}: {rows:?}");
        assert_eq!(
            guessed,
            ["false", "true", "false"],
            "{flag}: dat ran the guesser and found nothing"
        );
    }

    let off = fwdata_fixture_with_pattern_root(
        "guess-roots-off",
        "&lt;GuessRoots&gt;false&lt;/GuessRoots&gt;",
    );
    let (grammar, _) = load_grammar(&off.to_string_lossy()).expect("fixture loads");
    assert!(!grammar.guess_roots);
    let (rows, _) = run_pattern_batch(&off, "taad\n", &[]);
    assert_eq!(rows[0][4], "-", "the project's false turns guessing off");
    let (rows, _) = run_pattern_batch(&off, "taad\n", &["--guess"]);
    assert!(
        rows[0][4] != "-" && rows[0][5] == "true",
        "an explicit --guess overrides the project: {:?}",
        rows[0]
    );

    let err = run_batch(&[
        off.to_string_lossy().into_owned(),
        "w.txt".into(),
        "o.tsv".into(),
        "--guess=maybe".into(),
    ])
    .unwrap_err();
    assert!(err.contains("--guess=maybe"), "{err}");
}

/// End-to-end `--guess` gate through `run_batch` itself, covering both `--threads` writer paths, using the same synthetic lexical-pattern grammar shape as the engine-level guesser conformance gate.
mod guess_tests {
    use super::run_batch_tsv_custom;

    /// One lexical PATTERN entry ("[Any]*", matches every segment, so it is partitioned out of ordinary root lookup) plus one ordinary root ("kad") as the negative control.
    const GUESS_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>GuessCliTest</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>Verb</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRules="mrPast">
        <Name>Morphophonemic</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrPast" requiredPartsOfSpeech="posV">
            <Name>past_suffix</Name>
            <MorphemeId>PAST</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subPast">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="ePattern">
            <MorphemeId>PATTERN</MorphemeId>
            <Allomorphs><Allomorph id="aPattern"><PhoneticShape>[Any]*</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pattern</Gloss>
          </LexicalEntry>
          <LexicalEntry id="eKad" partOfSpeech="posV">
            <MorphemeId>KAD</MorphemeId>
            <Allomorphs><Allomorph id="aKad"><PhoneticShape>kad</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kad</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    /// Omitting `--guess` must keep the plain 5-column row shape and never analyze the out-of-lexicon word "gag", in both thread modes.
    #[test]
    fn guess_omitted_keeps_five_column_rows_and_finds_nothing_for_pattern_only_word() {
        for (tag, threads) in [("guess-off-seq", "1"), ("guess-off-par", "2")] {
            let lines =
                run_batch_tsv_custom(tag, GUESS_GRAMMAR_XML, "gag\n", &["--threads", threads]);
            assert_eq!(lines.len(), if threads == "1" { 2 } else { 1 }, "{lines:?}");
            let result_line = lines.last().unwrap();
            let fields: Vec<&str> = result_line.split('\t').collect();
            assert_eq!(
                fields.len(),
                5,
                "threads={threads}: --guess omitted must keep the pre-existing 5-column row: {fields:?}"
            );
            assert_eq!(fields[3], "ok");
            assert_eq!(
                fields[4], "-",
                "threads={threads}: guessing off must find nothing for the pattern-only word"
            );
        }
    }

    /// `--guess` analyzes the out-of-lexicon word "gag" (matched only by the lexical pattern) and appends a 6th `guessed` column marked `true`, in both thread modes.
    #[test]
    fn guess_flag_analyzes_pattern_only_word_and_marks_the_row_guessed() {
        for (tag, threads) in [("guess-on-seq", "1"), ("guess-on-par", "2")] {
            let lines = run_batch_tsv_custom(
                tag,
                GUESS_GRAMMAR_XML,
                "gag\n",
                &["--threads", threads, "--guess"],
            );
            let result_line = lines.last().unwrap();
            let fields: Vec<&str> = result_line.split('\t').collect();
            assert_eq!(
                fields.len(),
                6,
                "threads={threads}: --guess must append a 6th `guessed` column: {fields:?}"
            );
            assert_eq!(fields[3], "ok");
            assert_eq!(
                fields[4], "gag|gag",
                "threads={threads}: guessing on must analyze \"gag\" via the lexical pattern"
            );
            assert_eq!(
                fields[5], "true",
                "threads={threads}: a guessed analysis must be clearly marked, not silently \
                     indistinguishable from a confirmed one: {fields:?}"
            );
        }
    }

    /// Negative control: the ordinary lexical root "kad" is never marked guessed, since the guesser only fires on a genuine total miss.
    #[test]
    fn guess_flag_never_marks_the_ordinary_root_guessed() {
        for (tag, threads) in [("guess-control-seq", "1"), ("guess-control-par", "2")] {
            let lines = run_batch_tsv_custom(
                tag,
                GUESS_GRAMMAR_XML,
                "kad\n",
                &["--threads", threads, "--guess"],
            );
            let result_line = lines.last().unwrap();
            let fields: Vec<&str> = result_line.split('\t').collect();
            assert_eq!(fields.len(), 6, "{fields:?}");
            assert_eq!(fields[4], "KAD|kad");
            assert_eq!(
                fields[5], "false",
                "threads={threads}: an ordinary lexical hit must never be marked guessed: {fields:?}"
            );
        }
    }
}

#[test]
fn plain_parse_reports_invalid_shape_instead_of_complete_rejection() {
    let path = scratch_dir("parse-invalid-shape").join("grammar.xml");
    fs::write(&path, MINI_GRAMMAR_XML).unwrap();
    let error = super::run_parse(&[path.to_string_lossy().into_owned(), "q".into()])
        .expect_err("an unsegmentable word must not look like a complete rejection");
    assert!(error.contains("invalid_shape"), "{error}");
}

#[test]
fn plain_parse_reports_capped_search_and_preserves_complete_controls() {
    let path = scratch_dir("parse-completion").join("grammar.xml");
    fs::write(&path, homophonous_suffix_grammar_xml(7)).unwrap();
    let args = [
        path.to_string_lossy().into_owned(),
        format!("kad{}", "d".repeat(7)),
        "--work-cap".into(),
        "50000".into(),
    ];
    let error = super::run_parse(&args).expect_err("capped search must return an explicit failure");
    assert!(error.contains("capped"), "{error}");
    for word in ["kad", "dak"] {
        super::run_parse(&[path.to_string_lossy().into_owned(), word.into()]).unwrap();
    }
}

#[test]
fn work_cap_binds_both_batch_writer_paths() {
    for threads in ["1", "2"] {
        for flags in [vec!["--work-cap", "1"], vec!["--work-cap=1"]] {
            let mut args = vec!["--threads", threads];
            args.extend(flags);
            let rows = run_batch_tsv("independent-work-cap", &args);
            let fields: Vec<_> = rows.last().unwrap().split('\t').collect();
            assert_eq!(fields[3], "CAP");
            assert_eq!(fields[4], "-");
        }
    }
}

#[test]
fn work_cap_flag_binds_plain_parse_and_accepts_unbounded() {
    let dir = scratch_dir("parse-work-cap");
    let path = dir.join("grammar.xml");
    fs::write(&path, MINI_GRAMMAR_XML).unwrap();
    for flags in [vec!["--work-cap", "1"], vec!["--work-cap=1"]] {
        let mut args = vec![path.to_string_lossy().into_owned(), "kat".to_string()];
        args.extend(flags.into_iter().map(str::to_string));
        let err = super::run_parse(&args).expect_err("work cap must bound parsing");
        assert!(err.contains("capped"), "{err}");
    }
    super::run_parse(&[
        path.to_string_lossy().into_owned(),
        "kat".into(),
        "--work-cap=unbounded".into(),
    ])
    .expect("an unbounded work cap permits complete parsing");
    let rows = run_batch_tsv("work-unbounded", &["--work-cap", "unbounded"]);
    assert_eq!(rows.last().unwrap().split('\t').nth(3), Some("ok"));
}

#[test]
fn work_cap_flag_rejects_zero_before_file_access() {
    for flags in [vec!["--work-cap", "0"], vec!["--work-cap=0"]] {
        let mut args = vec![
            "unused.xml".into(),
            "unused.txt".into(),
            "unused.tsv".into(),
        ];
        args.extend(flags.into_iter().map(str::to_string));
        let err = run_batch(&args).expect_err("zero work cap must fail before file access");
        assert!(err.contains("--work-cap 0"), "{err}");
    }
}

/// The words a conformance `words.yaml` lists, in order.
fn fixture_words(fixture: &pg_conformance_fixtures::FixtureRef) -> Vec<String> {
    fs::read_to_string(fixture.words_yaml_path())
        .expect("read words.yaml")
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("- word:"))
        .map(|w| w.trim().trim_matches('"').to_string())
        .collect()
}

fn retire_report(
    tag: &str,
    grammar: &std::path::Path,
    words: &[String],
    ids: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let dir = scratch_dir(tag);
    let words_path = dir.join("words.txt");
    let out = dir.join("report.jsonl");
    fs::write(&words_path, words.join("\n")).expect("write words");
    let args: Vec<String> = [
        grammar.to_str().unwrap(),
        words_path.to_str().unwrap(),
        dir.join("out.tsv").to_str().unwrap(),
        "--retire",
        ids,
        "--retire-out",
        out.to_str().unwrap(),
        "--threads",
        "2",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    run_batch(&args)?;
    Ok(fs::read_to_string(out)
        .expect("read report")
        .lines()
        .map(|l| serde_json::from_str(l).expect("report row"))
        .collect())
}

/// The readings of each word, as sorted morph-list strings, from a plain `--analyses` run.
fn analysed_readings(
    tag: &str,
    grammar: &std::path::Path,
    words: &[String],
) -> Result<Vec<std::collections::BTreeSet<String>>, String> {
    let dir = scratch_dir(tag);
    let words_path = dir.join("words.txt");
    let analyses = dir.join("analyses.jsonl");
    fs::write(&words_path, words.join("\n")).expect("write words");
    let args: Vec<String> = [
        grammar.to_str().unwrap(),
        words_path.to_str().unwrap(),
        dir.join("out.tsv").to_str().unwrap(),
        "--analyses",
        analyses.to_str().unwrap(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    run_batch(&args)?;
    Ok(fs::read_to_string(analyses)
        .expect("read analyses")
        .lines()
        .map(|l| {
            let row: serde_json::Value = serde_json::from_str(l).unwrap();
            assert_eq!(row["unavailable"], serde_json::json!([]), "{row}");
            row["analyses"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a["morphs"].to_string())
                .collect()
        })
        .collect())
}

/// The snapshot of `grammar` with the named entries or allomorphs deleted by this test's own loop, written as a snapshot `.json`.
fn grammar_copy_without(grammar: &std::path::Path, ids: &[&str]) -> std::path::PathBuf {
    let mut snapshot = if grammar.extension().is_some_and(|e| e == "json") {
        pg_snapshot::Snapshot::from_json(&fs::read_to_string(grammar).unwrap()).unwrap()
    } else {
        pg_fwdata::import_file_lean(grammar).unwrap().0
    };
    let mut gone: Vec<String> = Vec::new();
    for entry in snapshot
        .lexicon
        .entries
        .iter()
        .filter(|e| ids.contains(&e.guid.as_str()))
    {
        gone.push(entry.guid.clone());
        gone.extend(entry.allomorphs.iter().map(|a| a.guid.clone()));
        gone.extend(entry.msas.iter().map(|m| m.guid().to_string()));
    }
    snapshot
        .lexicon
        .entries
        .retain(|e| !ids.contains(&e.guid.as_str()));
    for entry in &mut snapshot.lexicon.entries {
        gone.extend(
            entry
                .allomorphs
                .iter()
                .filter(|a| ids.contains(&a.guid.as_str()))
                .map(|a| a.guid.clone()),
        );
        entry.allomorphs.retain(|a| !ids.contains(&a.guid.as_str()));
    }
    // FieldWorks deleting an item removes it from every ad hoc prohibition, and a prohibition left without a key or other item goes too.
    snapshot.morphology.adhoc_prohibitions.retain_mut(|rule| {
        let (primary, others) = match rule {
            pg_snapshot::AdhocProhibition::Allomorph {
                primary, others, ..
            }
            | pg_snapshot::AdhocProhibition::Morpheme {
                primary, others, ..
            } => (primary, others),
        };
        others.retain(|o| !gone.contains(o));
        !gone.contains(primary) && !others.is_empty()
    });
    let path = scratch_dir("retire-oracle-copy").join("without.json");
    fs::write(&path, snapshot.to_json()).unwrap();
    path
}

/// `--retire` against a real parse of a grammar copy with the ids deleted: every row must match exactly.
fn assert_retire_matches_deleted_copy(
    tag: &str,
    grammar: &std::path::Path,
    words: &[String],
    ids: &[&str],
) -> Vec<serde_json::Value> {
    let baseline = analysed_readings(tag, grammar, words).expect("baseline parse");
    let copy = grammar_copy_without(grammar, ids);
    let deleted = analysed_readings(tag, &copy, words)
        .unwrap_or_else(|e| panic!("{tag} {ids:?}: deleted copy failed: {e}"));
    let report = retire_report(tag, grammar, words, &ids.join(","))
        .unwrap_or_else(|e| panic!("{tag} {ids:?}: --retire failed: {e}"));
    assert_eq!(report.len(), words.len());
    for (i, row) in report.iter().enumerate() {
        let lost: Vec<_> = baseline[i].difference(&deleted[i]).cloned().collect();
        let gained: Vec<_> = deleted[i].difference(&baseline[i]).cloned().collect();
        let expected = match (lost.is_empty(), gained.is_empty()) {
            (true, true) => "unchanged",
            (true, false) => "gained",
            (false, false) => "changed",
            (false, true) if deleted[i].is_empty() => "lost",
            (false, true) => "reduced",
        };
        let readings = |key: &str| -> std::collections::BTreeSet<String> {
            row[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["morphs"].to_string())
                .collect()
        };
        let ctx = format!("{tag} retire {ids:?} word {}: {row}", words[i]);
        assert_eq!(row["status"], expected, "{ctx}");
        assert_eq!(row["baselineReadings"], baseline[i].len(), "{ctx}");
        assert_eq!(row["retiredReadings"], deleted[i].len(), "{ctx}");
        assert_eq!(readings("lost"), lost.into_iter().collect(), "{ctx}");
        assert_eq!(readings("gained"), gained.into_iter().collect(), "{ctx}");
    }
    report
}

#[test]
fn retire_matches_a_real_parse_with_each_entry_deleted() {
    for name in [
        "adhoc-prohibition-circumfix-pairings",
        "adhoc-prohibition-variant-msa-copies",
        "adhoc-prohibition-stem-msa-copies",
    ] {
        let fixture = pg_conformance_fixtures::require_fixture("edge-cases", name);
        let project = fixture.dir.join("fieldworks").join("project.fwdata");
        let words = fixture_words(&fixture);
        assert!(!words.is_empty(), "{name}");
        let entries: Vec<String> = pg_fwdata::import_file_lean(&project)
            .unwrap()
            .0
            .lexicon
            .entries
            .iter()
            .map(|e| e.guid.clone())
            .collect();
        let mut changed = 0;
        for entry in &entries {
            let report = assert_retire_matches_deleted_copy(name, &project, &words, &[entry]);
            changed += report.iter().filter(|r| r["status"] != "unchanged").count();
        }
        let both: Vec<&str> = entries.iter().take(2).map(String::as_str).collect();
        assert_retire_matches_deleted_copy(name, &project, &words, &both);
        assert!(
            changed > 0,
            "{name}: retiring entries should change some word"
        );
    }
}

#[test]
fn retire_reports_lost_words_with_the_retired_entry_as_cause() {
    let fixture =
        pg_conformance_fixtures::require_fixture("edge-cases", "adhoc-prohibition-stem-msa-copies");
    let project = fixture.dir.join("fieldworks").join("project.fwdata");
    let words = fixture_words(&fixture);
    let entries = pg_fwdata::import_file_lean(&project)
        .unwrap()
        .0
        .lexicon
        .entries;
    let mut lost = 0;
    for entry in &entries {
        let rows = retire_report("retire-cause", &project, &words, &entry.guid).unwrap();
        for row in rows.iter().filter(|r| r["status"] == "lost") {
            lost += 1;
            assert_eq!(row["cause"]["kind"], "no-alternative-reading", "{row}");
            assert_eq!(
                row["cause"]["via"],
                serde_json::json!([entry.guid]),
                "{row}"
            );
            assert_eq!(row["reparsed"], true);
        }
    }
    assert!(lost > 0, "some retired entry must strand a word");
}

/// A snapshot whose one two-allomorph stem has an earlier allomorph that blocks the later one at the end of a word.
fn blocked_allomorph_snapshot() -> std::path::PathBuf {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pg-fwdata/tests/data/fixture.fwdata");
    let mut snapshot = pg_fwdata::import_file_lean(&source).unwrap().0;
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::Environment {
            guid: "00000000-0000-0000-0000-0000000000e1".into(),
            name: "wordFinal".into(),
            representation: "/_#".into(),
        });
    let entry = snapshot
        .lexicon
        .entries
        .iter_mut()
        .find(|e| e.guid.ends_with("0030"))
        .expect("the bare stem entry");
    let mut earlier = entry.allomorphs[0].clone();
    earlier.guid = "00000000-0000-0000-0000-0000000000e2".into();
    earlier.environments = vec!["00000000-0000-0000-0000-0000000000e1".into()];
    for ws in &mut earlier.forms {
        ws.form = "tta".into();
    }
    for ws in &mut entry.allomorphs[0].forms {
        ws.form = "dda".into();
    }
    entry.allomorphs.insert(0, earlier);
    // The import's own findings (a dangling environment, a provisional letter) are not part of this synthetic grammar.
    snapshot.conversion_provenance = pg_snapshot::ConversionProvenance::synthetic();
    for entry in &mut snapshot.lexicon.entries {
        for allomorph in &mut entry.allomorphs {
            allomorph.environments.retain(|e| !e.ends_with("00ff"));
        }
    }
    let path = scratch_dir("retire-blocked").join("blocked.json");
    fs::write(&path, snapshot.to_json()).unwrap();
    path
}

#[test]
fn retire_of_an_earlier_allomorph_unblocks_the_later_one() {
    let grammar = blocked_allomorph_snapshot();
    let words: Vec<String> = ["tta", "dda"].iter().map(|s| s.to_string()).collect();
    let baseline = analysed_readings("retire-blocked-base", &grammar, &words).unwrap();
    assert_eq!(baseline[0].len(), 1, "the earlier allomorph parses tta");
    assert_eq!(baseline[1].len(), 0, "the earlier allomorph blocks dda");
    let report = assert_retire_matches_deleted_copy(
        "retire-blocked",
        &grammar,
        &words,
        &["00000000-0000-0000-0000-0000000000e2"],
    );
    assert_eq!(report[0]["status"], "lost");
    assert_eq!(
        report[0]["cause"]["via"],
        serde_json::json!(["00000000-0000-0000-0000-0000000000e2"])
    );
    assert_eq!(report[1]["status"], "gained");
    assert_eq!(report[1]["reparsed"], true);
}

#[test]
fn retire_refuses_what_it_cannot_evaluate() {
    let fixture =
        pg_conformance_fixtures::require_fixture("edge-cases", "adhoc-prohibition-stem-msa-copies");
    let project = fixture.dir.join("fieldworks").join("project.fwdata");
    let words = vec!["da".to_string()];
    let err = retire_report("retire-unknown", &project, &words, "no-such-guid").unwrap_err();
    assert!(err.contains("neither an entry nor an allomorph"), "{err}");
    let err = retire_report("retire-xml", &fixture.grammar_path(), &words, "x").unwrap_err();
    assert!(err.contains("entry guids"), "{err}");
}

#[test]
fn retire_of_entries_named_by_ad_hoc_prohibitions_drops_the_reference_and_parses() {
    let fixture =
        pg_conformance_fixtures::require_fixture("edge-cases", "adhoc-prohibition-stem-msa-copies");
    let project = fixture.dir.join("fieldworks").join("project.fwdata");
    let words = fixture_words(&fixture);
    let snapshot = pg_fwdata::import_file_lean(&project).unwrap().0;
    let named: std::collections::BTreeSet<String> = snapshot
        .morphology
        .adhoc_prohibitions
        .iter()
        .flat_map(|rule| match rule {
            pg_snapshot::AdhocProhibition::Allomorph {
                primary, others, ..
            }
            | pg_snapshot::AdhocProhibition::Morpheme {
                primary, others, ..
            } => std::iter::once(primary)
                .chain(others.iter())
                .cloned()
                .collect::<Vec<_>>(),
        })
        .collect();
    let owners: Vec<String> = snapshot
        .lexicon
        .entries
        .iter()
        .filter(|e| {
            e.allomorphs.iter().any(|a| named.contains(&a.guid))
                || e.msas.iter().any(|m| named.contains(m.guid()))
        })
        .map(|e| e.guid.clone())
        .collect();
    assert!(
        !owners.is_empty(),
        "the fixture names entries in prohibitions"
    );
    for owner in &owners {
        assert_retire_matches_deleted_copy("retire-adhoc", &project, &words, &[owner]);
    }
    let allomorph = snapshot
        .lexicon
        .entries
        .iter()
        .flat_map(|e| e.allomorphs.iter())
        .find(|a| named.contains(&a.guid))
        .map(|a| a.guid.clone());
    if let Some(allomorph) = allomorph {
        assert_retire_matches_deleted_copy("retire-adhoc-allo", &project, &words, &[&allomorph]);
    }
}

#[test]
fn retire_out_refuses_the_grammar_the_word_list_and_the_tsv() {
    let fixture =
        pg_conformance_fixtures::require_fixture("edge-cases", "adhoc-prohibition-stem-msa-copies");
    let project = fixture.dir.join("fieldworks").join("project.fwdata");
    let guid = pg_fwdata::import_file_lean(&project)
        .unwrap()
        .0
        .lexicon
        .entries[0]
        .guid
        .clone();
    let dir = scratch_dir("retire-collide");
    let grammar_copy = dir.join("grammar.fwdata");
    fs::copy(&project, &grammar_copy).unwrap();
    let words = dir.join("words.txt");
    fs::write(&words, "da\n").unwrap();
    let tsv = dir.join("out.tsv");
    let before = fs::read(&grammar_copy).unwrap();
    for (label, target) in [
        ("grammar", grammar_copy.clone()),
        ("word list", words.clone()),
        ("TSV output", tsv.clone()),
        ("grammar", dir.join("sub/../grammar.fwdata")),
    ] {
        fs::create_dir_all(dir.join("sub")).unwrap();
        let args: Vec<String> = [
            grammar_copy.to_str().unwrap(),
            words.to_str().unwrap(),
            tsv.to_str().unwrap(),
            "--retire",
            &guid,
            "--retire-out",
            target.to_str().unwrap(),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let err = run_batch(&args).unwrap_err();
        assert!(err.contains(&format!("cannot overwrite {label}")), "{err}");
    }
    assert_eq!(fs::read(&grammar_copy).unwrap(), before);
    assert_eq!(fs::read_to_string(&words).unwrap(), "da\n");
}

#[test]
fn retire_cannot_compare_a_word_with_an_unprojectable_reading() {
    let parsed = |unprojected| crate::retire::Parsed {
        readings: Vec::new(),
        incomplete: false,
        unprojected,
    };
    let (_, lost, gained, reason) = crate::retire::compare(&parsed(0), Some(&parsed(1)));
    assert!(lost.is_empty() && gained.is_empty());
    assert!(reason.is_some_and(|r| r.contains("cannot be projected")));
    let (_, _, _, reason) = crate::retire::compare(&parsed(1), Some(&parsed(0)));
    assert!(reason.is_some());
    let (_, _, _, reason) = crate::retire::compare(&parsed(0), Some(&parsed(0)));
    assert!(reason.is_none());
}

/// The stem-msa-copies grammar with its prohibitions replaced by one that keeps `ta` away from the co-occurrence of `da` and `ba`.
fn multi_member_prohibition_snapshot(morpheme_level: bool) -> (std::path::PathBuf, String, String) {
    let fixture =
        pg_conformance_fixtures::require_fixture("edge-cases", "adhoc-prohibition-stem-msa-copies");
    let project = fixture.dir.join("fieldworks").join("project.fwdata");
    let mut snapshot = pg_fwdata::import_file_lean(&project).unwrap().0;
    let find = |form: &str| {
        snapshot
            .lexicon
            .entries
            .iter()
            .find(|e| e.allomorphs[0].forms.iter().any(|f| f.form == form))
            .unwrap_or_else(|| panic!("entry {form}"))
            .clone()
    };
    let (ta, da, ba) = (find("ta"), find("da"), find("ba"));
    let pick = |e: &pg_snapshot::LexEntry| {
        if morpheme_level {
            e.msas[0].guid().to_string()
        } else {
            e.allomorphs[0].guid.clone()
        }
    };
    let (primary, others) = (pick(&ta), vec![pick(&da), pick(&ba)]);
    let guid = "00000000-0000-0000-0000-0000000000a1".to_string();
    snapshot.morphology.adhoc_prohibitions = vec![if morpheme_level {
        pg_snapshot::AdhocProhibition::Morpheme {
            guid,
            disabled: false,
            primary,
            others,
            adjacency: pg_snapshot::Adjacency::Anywhere,
        }
    } else {
        pg_snapshot::AdhocProhibition::Allomorph {
            guid,
            disabled: false,
            primary,
            others,
            adjacency: pg_snapshot::Adjacency::Anywhere,
        }
    }];
    snapshot.morphology.adhoc_prohibition_groups = None;
    let path = scratch_dir("retire-multi").join(format!("multi-{morpheme_level}.json"));
    fs::write(&path, snapshot.to_json()).unwrap();
    (path, da.guid.clone(), da.allomorphs[0].guid.clone())
}

#[test]
fn retire_that_shrinks_a_surviving_prohibition_reparses_words_that_never_used_the_item() {
    let words: Vec<String> = ["taba", "tada", "ta", "da", "ba", "tadaba", "tabada"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    for morpheme_level in [false, true] {
        let (grammar, da_entry, da_allomorph) = multi_member_prohibition_snapshot(morpheme_level);
        let baseline = analysed_readings("retire-multi-base", &grammar, &words).unwrap();
        assert!(!baseline[0].is_empty(), "taba parses before the retire");
        // An allomorph retire leaves a morpheme-level prohibition's MSA reference intact.
        let ids = if morpheme_level {
            vec![da_entry]
        } else {
            vec![da_entry, da_allomorph]
        };
        for id in ids {
            let tag = format!("retire-multi-{morpheme_level}");
            let report = assert_retire_matches_deleted_copy(&tag, &grammar, &words, &[&id]);
            // The shrunk prohibition fires on ta+ba alone, so taba is lost though it never used da.
            assert_eq!(report[0]["status"], "lost", "{}", report[0]);
            assert_eq!(report[0]["reparsed"], true);
        }
    }
}

#[test]
fn retire_of_the_only_affix_in_an_obligatory_slot_reparses_the_bare_stems() {
    let fixture =
        pg_conformance_fixtures::require_fixture("edge-cases", "empty-required-template-slot");
    let project = fixture.dir.join("fieldworks").join("project.fwdata");
    let mut snapshot = pg_fwdata::import_file_lean(&project).unwrap().0;
    let slot = "469a6957-8f7a-4ecc-bbf4-434978045a96";
    let mut flipped = false;
    for pos in &mut snapshot.morphology.parts_of_speech {
        for s in pos.affix_slots.iter_mut().filter(|s| s.guid == slot) {
            s.optional = false;
            flipped = true;
        }
    }
    assert!(flipped, "the suffix slot exists");
    let grammar = scratch_dir("retire-slot").join("slot.json");
    fs::write(&grammar, snapshot.to_json()).unwrap();
    let words: Vec<String> = ["monu", "monua", "amonu"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let baseline = analysed_readings("retire-slot-base", &grammar, &words).unwrap();
    assert!(
        baseline[0].is_empty(),
        "the required slot blocks the bare stem"
    );
    assert!(!baseline[1].is_empty());
    // The retired affix was the slot's only filler, so the compiler drops the slot and the bare stem parses.
    let report = assert_retire_matches_deleted_copy(
        "retire-slot",
        &grammar,
        &words,
        &["08d86318-7f90-43bf-8629-b859e36bd67d"],
    );
    assert_eq!(report[0]["status"], "gained", "{}", report[0]);
    assert_eq!(report[0]["reparsed"], true);
}

fn without_timing(rows: &[serde_json::Value]) -> Vec<serde_json::Value> {
    rows.iter()
        .cloned()
        .map(|mut row| {
            row.as_object_mut().unwrap().remove("elapsedMs");
            row
        })
        .collect()
}

#[test]
fn query_fields_are_opt_in_and_name_entry_signature_and_failure() {
    let fixture = fwdata_fixture_with_k();
    let plain = run_batch_sidecar_path("qf-plain", &fixture, "kad\nkat\n", &[]);
    assert!(plain[0].get("readingCount").is_none() && plain[0].get("failure").is_none());
    assert!(plain[0]["analyses"][0]["morphs"][0]
        .get("signature")
        .is_none());
    let rows = run_batch_sidecar_path("qf", &fixture, "kad\nkat\n", &["--query-fields"]);
    let morph = &rows[0]["analyses"][0]["morphs"][0];
    assert_eq!(rows[0]["readingCount"], 1);
    assert_eq!(rows[0]["failure"], serde_json::Value::Null);
    assert_eq!(morph["form"], plain[0]["analyses"][0]["morphs"][0]["form"]);
    assert_eq!(morph["entry"], "00000000-0000-0000-0000-000000000030");
    let signature = format!(
        "{}:{}",
        morph["entry"].as_str().unwrap(),
        morph["msa"].as_str().unwrap()
    );
    assert_eq!(morph["signature"], signature.as_str());
    assert_eq!(rows[0]["analyses"][0]["signature"], signature.as_str());
    assert_eq!(rows[1]["readingCount"], 0);
    assert_eq!(
        rows[1]["failure"],
        serde_json::json!({"status": "noParse", "workSteps": rows[1]["failure"]["workSteps"], "workCap": 5_000_000_000u64})
    );
    let capped = run_batch_sidecar_path(
        "qf-cap",
        &fixture,
        "kad\n",
        &["--query-fields", "--work-cap", "1"],
    );
    assert_eq!(capped[0]["capped"], true);
    assert_eq!(capped[0]["failure"]["status"], "workCap");
    assert_eq!(capped[0]["failure"]["workCap"], 1);
}

#[test]
fn query_fields_are_refused_without_a_sidecar() {
    let fixture = fwdata_fixture_with_k();
    let dir = scratch_dir("qf-refused");
    let words = dir.join("words.txt");
    fs::write(&words, "kat\n").unwrap();
    let error = run_batch(&[
        fixture.to_string_lossy().into_owned(),
        words.to_string_lossy().into_owned(),
        dir.join("out.tsv").to_string_lossy().into_owned(),
        "--query-fields".to_string(),
    ])
    .expect_err("refused");
    assert!(
        error.contains("--query-fields requires --analyses"),
        "{error}"
    );
}

/// Signatures come from source guids only, so they repeat across thread counts and give a circumfix's halves one signature.
#[test]
fn query_fields_are_deterministic_across_threads_and_share_a_circumfix_signature() {
    let fixture = pg_conformance_fixtures::require_fixture(
        "edge-cases",
        "adhoc-prohibition-circumfix-pairings",
    );
    let project = fixture.dir.join("fieldworks").join("project.fwdata");
    let args = ["--query-fields", "--morph-spans"];
    let one = run_batch_sidecar_path(
        "qf-t1",
        &project,
        "tagaka\ntogaki\nzzz\n",
        &[&args[..], &["--threads", "1"]].concat(),
    );
    let four = run_batch_sidecar_path(
        "qf-t4",
        &project,
        "tagaka\ntogaki\nzzz\n",
        &[&args[..], &["--threads", "4"]].concat(),
    );
    assert_eq!(without_timing(&one), without_timing(&four));
    let readings: Vec<&serde_json::Value> = one
        .iter()
        .flat_map(|r| r["analyses"].as_array().unwrap())
        .collect();
    assert!(!readings.is_empty());
    let mut halves = 0;
    for reading in readings {
        let morphs = reading["morphs"].as_array().unwrap();
        for (i, a) in morphs.iter().enumerate() {
            for b in &morphs[i + 1..] {
                if a["entry"].is_string() && a["entry"] == b["entry"] && a["form"] != b["form"] {
                    assert_eq!(a["signature"], b["signature"]);
                    halves += 1;
                }
            }
        }
    }
    assert!(halves > 0, "the fixture should give a circumfix reading");
}

struct CacheRun {
    parsed: usize,
    tsv: String,
    sidecar: String,
}

fn cached_run(
    dir: &std::path::Path,
    grammar: &std::path::Path,
    words: &str,
    extra: &[&str],
) -> CacheRun {
    let words_path = dir.join("words.txt");
    fs::write(&words_path, words).unwrap();
    let mut args: Vec<String> = vec![
        grammar.to_string_lossy().into_owned(),
        words_path.to_string_lossy().into_owned(),
        dir.join("out.tsv").to_string_lossy().into_owned(),
        "--analyses".to_string(),
        dir.join("a.jsonl").to_string_lossy().into_owned(),
        "--result-cache".to_string(),
        dir.join("cache").to_string_lossy().into_owned(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    let parsed = run_batch_counted(&args).unwrap_or_else(|e| panic!("run_batch failed: {e}"));
    CacheRun {
        parsed,
        tsv: fs::read_to_string(dir.join("out.tsv")).unwrap(),
        sidecar: fs::read_to_string(dir.join("a.jsonl")).unwrap(),
    }
}

#[test]
fn result_cache_replays_unchanged_rows_and_misses_when_anything_that_changes_output_does() {
    let fixture = fwdata_fixture_with_k();
    let dir = scratch_dir("cache");
    let cold = cached_run(
        &dir,
        &fixture,
        "kad\nkat\nkad\n",
        &["--query-fields", "--threads", "1"],
    );
    assert_eq!(cold.parsed, 2, "a repeated word is parsed once");
    let plain = run_batch_sidecar_path(
        "cache-plain",
        &fixture,
        "kad\nkat\nkad\n",
        &["--query-fields"],
    );
    let cached_rows: Vec<serde_json::Value> = cold
        .sidecar
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(without_timing(&cached_rows), without_timing(&plain));
    assert_eq!(cold.tsv.lines().count(), 3);
    // Same bytes, no parse, whatever the thread count.
    for threads in ["1", "4"] {
        let warm = cached_run(
            &dir,
            &fixture,
            "kad\nkat\nkad\n",
            &["--query-fields", "--threads", threads],
        );
        assert_eq!(warm.parsed, 0);
        assert_eq!((&warm.tsv, &warm.sidecar), (&cold.tsv, &cold.sidecar));
    }
    // A new word parses only itself, and a reordered list reindexes.
    let more = cached_run(&dir, &fixture, "kat\nkats\nkad\n", &["--query-fields"]);
    assert_eq!(more.parsed, 1);
    assert!(more.tsv.starts_with("0\tkat\t"));
    let back = cached_run(&dir, &fixture, "kats\n", &["--query-fields"]);
    assert_eq!(back.parsed, 0);
    // Each option that changes the output is a miss.
    for changed in [
        vec![],
        vec!["--query-fields", "--work-cap", "7"],
        vec!["--query-fields", "--word-mem-cap-mib", "50"],
        vec!["--query-fields", "--morph-spans"],
        vec!["--query-fields", "--guess=false"],
        vec!["--query-fields", "--work-columns"],
        vec!["--query-fields", "--always-enforce-final-templates=true"],
    ] {
        let run = cached_run(&dir, &fixture, "kad\n", &changed);
        assert_eq!(run.parsed, 1, "{changed:?} must miss");
        let again = cached_run(&dir, &fixture, "kad\n", &changed);
        assert_eq!(again.parsed, 0, "{changed:?} must then hit");
    }
    // The grammar's content, not its path or time, is the key.
    let edited = dir.join("edited.fwdata");
    fs::write(
        &edited,
        format!("{}\n", fs::read_to_string(&fixture).unwrap()),
    )
    .unwrap();
    assert_eq!(
        cached_run(&dir, &edited, "kad\n", &["--query-fields"]).parsed,
        1
    );
    let copy = dir.join("copy.fwdata");
    fs::copy(&fixture, &copy).unwrap();
    assert_eq!(
        cached_run(&dir, &copy, "kad\n", &["--query-fields"]).parsed,
        0
    );
}

#[test]
fn result_cache_is_refused_where_rows_cannot_be_replayed_and_is_size_bounded() {
    let fixture = fwdata_fixture_with_k();
    let dir = scratch_dir("cache-refused");
    let words = dir.join("words.txt");
    fs::write(&words, "kat\n").unwrap();
    let base = |extra: &[&str]| -> Vec<String> {
        let mut args: Vec<String> = vec![
            fixture.to_string_lossy().into_owned(),
            words.to_string_lossy().into_owned(),
            dir.join("out.tsv").to_string_lossy().into_owned(),
            "--analyses".to_string(),
            dir.join("a.jsonl").to_string_lossy().into_owned(),
            "--result-cache".to_string(),
            dir.join("cache").to_string_lossy().into_owned(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        args
    };
    for extra in [
        ["--stats"],
        ["--partials=1"],
        ["--cost-column"],
        ["--prefilter=on"],
    ] {
        assert!(run_batch(&base(&extra)).is_err(), "{extra:?}");
    }
    // A least recently used grammar's directory goes once the bound is passed.
    let old = dir.join("cache").join("old-key");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("seg-1.jsonl"), vec![b'x'; 2 << 20]).unwrap();
    run_batch(&base(&["--result-cache-max-mib", "1"])).unwrap();
    assert!(!old.exists());
    let kept = fs::read_dir(dir.join("cache")).unwrap().count();
    assert_eq!(kept, 1);
}

/// A variant's morph names the variant's own entry, while its `msa` is the main entry's.
#[test]
fn query_fields_entry_of_a_variant_is_the_variant_and_its_msa_is_the_main_entrys() {
    let fixture = pg_conformance_fixtures::require_fixture(
        "edge-cases",
        "adhoc-prohibition-variant-msa-copies",
    );
    let project = fixture.dir.join("fieldworks").join("project.fwdata");
    let rows = run_batch_sidecar_path("qf-variant", &project, "da\ndi\n", &["--query-fields"]);
    let first_morph = |row: &serde_json::Value| -> Vec<(String, String)> {
        row["analyses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let m = &r["morphs"][0];
                (
                    m["entry"].as_str().expect("entry").to_string(),
                    m["msa"].as_str().expect("msa").to_string(),
                )
            })
            .collect()
    };
    let (main, variant) = (first_morph(&rows[0]), first_morph(&rows[1]));
    assert!(!main.is_empty() && !variant.is_empty());
    for (entry, msa) in &variant {
        assert!(
            main.iter().all(|(main_entry, _)| main_entry != entry),
            "the variant has its own entry"
        );
        assert!(
            main.iter().any(|(_, main_msa)| main_msa == msa),
            "the variant's msa is the main entry's"
        );
    }
}

/// A cached run refuses to overwrite the grammar or the word list, like an uncached one.
#[test]
fn result_cache_refuses_outputs_that_would_overwrite_inputs() {
    let fixture = fwdata_fixture_with_k();
    let dir = scratch_dir("cache-collide");
    let words = dir.join("words.txt");
    fs::write(&words, "kat\n").unwrap();
    let cache = dir.join("cache");
    let run = |out: &std::path::Path, analyses: &std::path::Path| {
        run_batch_counted(&[
            fixture.to_string_lossy().into_owned(),
            words.to_string_lossy().into_owned(),
            out.to_string_lossy().into_owned(),
            "--analyses".into(),
            analyses.to_string_lossy().into_owned(),
            "--result-cache".into(),
            cache.to_string_lossy().into_owned(),
        ])
    };
    let before = fs::read(&fixture).unwrap();
    for (out, analyses) in [
        (dir.join("o.tsv"), fixture.clone()),
        (dir.join("o.tsv"), words.clone()),
        (fixture.clone(), dir.join("a.jsonl")),
        (words.clone(), dir.join("a.jsonl")),
        (dir.join("same"), dir.join("same")),
    ] {
        let error = run(&out, &analyses).expect_err("must refuse");
        assert!(error.contains("cannot overwrite"), "{error}");
    }
    assert_eq!(fs::read(&fixture).unwrap(), before);
    assert_eq!(fs::read_to_string(&words).unwrap(), "kat\n");
}

/// The key follows the executable's and the grammar's bytes and every option.
#[test]
fn result_cache_key_changes_with_the_executable_the_grammar_and_the_options() {
    let key = crate::batch_result_cache::cache_key;
    let base = key("g1", "exe1", &["--morph-spans".to_string()]);
    assert_eq!(base, key("g1", "exe1", &["--morph-spans".to_string()]));
    assert_ne!(base, key("g1", "exe2", &["--morph-spans".to_string()]));
    assert_ne!(base, key("g2", "exe1", &["--morph-spans".to_string()]));
    assert_ne!(base, key("g1", "exe1", &[]));
}
