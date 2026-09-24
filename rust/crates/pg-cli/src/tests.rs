//! `--word-timeout-ms` end-to-end plumbing: flag parsing, `Morpher` wiring, and the TSV row
//! shape, exercised through `run_batch` itself (not just `Morpher::parse_word` --
//! `pg-parse/tests/word_timeout_gate.rs` already covers the engine-level behavior) so a bug in
//! this file's own flag parsing or row-writing can't hide behind a lower-level test passing.
//! Covers both `--threads` writer paths per the task brief -- the sequential (`STARTED` +
//! per-line flush) and rayon-parallel (buffered, no `STARTED`) modes have genuinely different
//! code paths in `run_batch` and each needed its own bug fixed above.
use super::{
    deduplicate_warnings, load_grammar, run_batch, write_parse_analysis_row, StepCap,
    DEFAULT_STEP_CAP,
};
use std::fs;
use std::sync::atomic::{AtomicU32, Ordering};

/// A minimal, self-contained grammar (no phonological/morphological rules): one stratum, one table, one `LexicalEntry` whose surface form is "kat" — root-only lookup is enough to exercise the batch pipeline.
const MINI_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
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
    let rows = run_batch_sidecar_path("analyses-fwdata-source-guids", &fixture, "kat\n", &[]);
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row["invalidShape"], false);
    assert_eq!(row["unavailable"], serde_json::json!([]));
    assert_eq!(row["analyses"].as_array().unwrap().len(), 1);
    let morph = row["analyses"][0]["morphs"][0]
        .as_object()
        .expect("the fwdata fixture analysis should project");
    assert_eq!(morph["form"], "00000000-0000-0000-0000-000000000031");
    assert_eq!(morph["msa"], "00000000-0000-0000-0000-000000000032");
    assert_eq!(morph["inflType"], serde_json::Value::Null);
    assert_eq!(morph["guessedString"], serde_json::Value::Null);
}

#[test]
fn analyses_sidecar_preserves_cap_and_timeout_flags() {
    let grammar_xml = homophonous_suffix_grammar_xml(7);
    let capped_rows = run_batch_sidecar_custom(
        "analyses-cap-timeout",
        &grammar_xml,
        &format!("kad{}\n", "d".repeat(7)),
        &["--step-cap", "500"],
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
        analyses: vec![("partial".into(), "kat".into())],
        structured: Vec::new(),
        capped: true,
        invalid_shape: false,
        steps: 1,
        timed_out: true,
        guessed: false,
        candidates_generated: 0,
    };
    let mut encoded = Vec::new();
    write_parse_analysis_row(&mut encoded, 0, "kat", 12, &outcome, &grammar)
        .expect("serialize mixed-bound outcome");
    let row: serde_json::Value = serde_json::from_slice(&encoded).expect("valid row");
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

/// `k` homophonous one-shot suffix rules unapplying "d", so the unwind is genuinely combinatorial; `MINI_GRAMMAR_XML`'s root-only lookup takes zero steps and cannot exercise a real `--step-cap` firing.
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
    <Name>StepCapCliTest</Name>
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

/// A small `--step-cap` fires on a genuinely combinatorial unwind, in both thread modes; an incomplete outcome is typed `CAP`, never an `ok` row.
#[test]
fn step_cap_small_writes_cap_row_both_thread_modes() {
    let grammar_xml = homophonous_suffix_grammar_xml(7);
    let word = format!("kad{}\n", "d".repeat(7));
    for (tag, threads) in [("seq-cap", "1"), ("par-cap", "2")] {
        let lines = run_batch_tsv_custom(
            tag,
            &grammar_xml,
            &word,
            &["--step-cap", "500", "--threads", threads],
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

#[test]
fn default_step_cap_is_fifty_million() {
    assert_eq!(
        DEFAULT_STEP_CAP,
        StepCap::Finite(std::num::NonZeroU64::new(50_000_000).unwrap())
    );
}

/// `--step-cap 0` is rejected before any file is touched, with the same message `StepCap::from_str` gives.
#[test]
fn step_cap_zero_is_rejected_with_a_specific_message() {
    let err = run_batch(&[
        "unused.xml".to_string(),
        "unused.txt".to_string(),
        "unused.tsv".to_string(),
        "--step-cap".to_string(),
        "0".to_string(),
    ])
    .expect_err("--step-cap 0 must be rejected");
    assert!(err.contains("fires before the first step"), "{err}");
}

/// `--step-cap unbounded` opts back into no bound at all; "kat" analyzes in far fewer than `DEFAULT_STEP_CAP` steps regardless, so this only proves the flag parses and the run still completes normally.
#[test]
fn step_cap_unbounded_parses_and_batch_completes_ok() {
    let lines = run_batch_tsv("unbounded-ok", &["--step-cap", "unbounded"]);
    let result_line = lines.last().expect("at least one line");
    let fields: Vec<&str> = result_line.split('\t').collect();
    assert_eq!(fields[3], "ok", "{fields:?}");
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

    assert_eq!(unknown_morph_type.len(), 1);
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

#[test]
fn warning_deduplication_uses_code_and_source_identity() {
    use pg_snapshot::{FwClass, FwObjectRef, Warning};

    let same_fact = || {
        Warning::new("fwdata.example", "different layer wording").with_subject(
            FwObjectRef::new(FwClass::MoForm)
                .guid("00000000-0000-0000-0000-000000000044")
                .name("xxx"),
        )
    };
    let other_object = Warning::new("fwdata.example", "other allomorph").with_subject(
        FwObjectRef::new(FwClass::MoForm).guid("00000000-0000-0000-0000-000000000045"),
    );
    let other_code = Warning::new("fwdata.other", "different issue").with_subject(
        FwObjectRef::new(FwClass::MoForm).guid("00000000-0000-0000-0000-000000000044"),
    );

    let unique = deduplicate_warnings(vec![same_fact(), same_fact(), other_object, other_code]);
    assert_eq!(unique.len(), 3);
    assert_eq!(unique[0].message, "different layer wording");
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
