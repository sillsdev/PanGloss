/// Sums the displayed rows for its own denominators -- correct only where the caller shows every matched row.
fn render_narrow_self_totals(
    rows: &[RowView],
    wide: bool,
) -> (Vec<&'static str>, Vec<Vec<String>>) {
    render_narrow(rows, wide, &Denominators::of(rows), &[])
}
use super::*;
use std::fs;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

fn scratch_dir(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "pangloss-stats-cmd-test-{tag}-{}-{n}",
        std::process::id()
    ));
    // Windows reuses pids, so a previous run's cache can still sit here and reopen with its engine.
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Drives `run_stats` directly rather than scraping its source for matching literals.
mod flag_spec_drives_the_parser {
    use crate::stats_cmd::run_stats;
    use crate::surface::find_command;

    /// A scratch `--cache` path so `run_stats` reaches argument validation for every flag below.
    fn base_args(cache_dir: &std::path::Path) -> Vec<String> {
        vec![
            "irrelevant-project".to_string(),
            "--cache".to_string(),
            cache_dir
                .join("cache.sqlite3")
                .to_string_lossy()
                .into_owned(),
        ]
    }

    #[test]
    fn unknown_flag_is_rejected_with_the_shared_message() {
        let dir = super::scratch_dir("stats-unknown-flag");
        let mut args = base_args(&dir);
        args.push("--bogus-flag".to_string());
        let error = run_stats(&args).expect_err("an undeclared flag must be rejected");
        assert_eq!(error, "unknown option: --bogus-flag");
    }

    /// Drives `run_stats` with each declared flag; acceptance means "not rejected as unknown".
    #[test]
    fn every_declared_flag_is_accepted_by_run_stats() {
        let spec = find_command("stats").expect("stats must be in COMMANDS");
        for flag in spec.flags {
            if flag.name == "--cache" {
                continue; // already exercised by every call via base_args
            }
            let dir = super::scratch_dir("stats-declared-flag");
            let mut args = base_args(&dir);
            args.push(flag.name.to_string());
            if flag.takes_value {
                let value = match flag.name {
                    "--group" => "object",
                    "--direction" => "analysis",
                    "--sort" => "time",
                    "--format" => "text",
                    "--top" => "1",
                    _ => "x",
                };
                args.push(value.to_string());
            }
            let result = run_stats(&args);
            assert!(
                !matches!(&result, Err(e) if e.starts_with("unknown option:")),
                "{}: expected the parser to accept this declared flag, got {result:?}",
                flag.name
            );
        }
    }
}

/// One conformance fixture's grammar plus a word guaranteed not `expect_skip`.
fn fixture_grammar_and_word(category: &str, name: &str) -> (String, String) {
    let f = pg_conformance_fixtures::require_fixture(category, name);
    let words = f.load_words_yaml();
    let word = words
        .words
        .iter()
        .find(|w| !w.expect_skip)
        .expect("fixture must have at least one non-skip word")
        .word
        .clone();
    (f.load_grammar_xml(), word)
}

/// Exercises at least one morphological/phonological rule, unlike a bare-root grammar.
fn primary_fixture() -> (String, String) {
    fixture_grammar_and_word("languages", "metathesis-phase-isolation")
}

#[test]
fn final_template_stats_line_is_deterministic_for_empty_and_nonempty_rows() {
    assert_eq!(
        final_template_stats_line(pg_rules::stats::PruneCounters::default()),
        "FINAL_TEMPLATE_STATS_NEWLY_ANALYZED\ttemplate_entries=0\ttemplate_batteries_skipped=0\tfinal_templates_skipped=0"
    );
    assert_eq!(
        final_template_stats_line(pg_rules::stats::PruneCounters {
            template_entries: 2,
            template_batteries_skipped: 3,
            final_templates_skipped: 5,
        }),
        "FINAL_TEMPLATE_STATS_NEWLY_ANALYZED\ttemplate_entries=2\ttemplate_batteries_skipped=3\tfinal_templates_skipped=5"
    );
}

fn seed_policy_cache(path: &std::path::Path, options_json: &str) -> pg_stats::StatsCache {
    let mut outcome = pg_stats::StatsCache::open(path, "policy-hash").unwrap();
    let run = pg_stats::RunMetadata {
        build_info: "test".to_string(),
        fwdata_path: "fixture.xml".to_string(),
        grammar_hash: "policy-hash".to_string(),
        engine: "hc".to_string(),
        options_hash: "options".to_string(),
        options_json: options_json.to_string(),
        created_utc: "unix:0".to_string(),
        step_cap: None,
    };
    outcome.cache.flush(&run, &[]).unwrap();
    outcome.cache
}

#[test]
fn final_template_policy_cache_treats_legacy_missing_field_as_false() {
    let dir = scratch_dir("policy-legacy");
    let path = dir.join("cache.sqlite3");
    let cache = seed_policy_cache(&path, "{}");
    refuse_if_final_template_policy_differs(&cache, &path, false)
        .expect("legacy rows must remain compatible with the default policy");
    let err = refuse_if_final_template_policy_differs(&cache, &path, true)
        .expect_err("legacy rows must refuse the result-changing policy");
    assert!(err.contains(path.to_str().unwrap()), "error: {err}");
    assert!(
        err.contains("false") && err.contains("true"),
        "error: {err}"
    );
}

#[test]
fn final_template_policy_cache_rejects_malformed_prior_options() {
    for (name, json, expected) in [
        ("syntax", "{", "malformed prior options JSON"),
        ("non-object", "[]", "not an object"),
        (
            "non-boolean",
            r#"{"always_enforce_final_templates":"yes"}"#,
            "non-boolean always_enforce_final_templates",
        ),
    ] {
        let dir = scratch_dir(&format!("policy-malformed-{name}"));
        let path = dir.join("cache.sqlite3");
        let cache = seed_policy_cache(&path, json);
        let err = refuse_if_final_template_policy_differs(&cache, &path, false)
            .expect_err("invalid cached options must fail loudly");
        assert!(err.contains(path.to_str().unwrap()), "error: {err}");
        assert!(err.contains(expected), "error: {err}");
    }
}

#[test]
fn final_template_policies_accept_separate_caches() {
    let dir = scratch_dir("policy-separate");
    let default_path = dir.join("default.sqlite3");
    let override_path = dir.join("override.sqlite3");
    let default_cache =
        seed_policy_cache(&default_path, r#"{"always_enforce_final_templates":false}"#);
    let override_cache =
        seed_policy_cache(&override_path, r#"{"always_enforce_final_templates":true}"#);
    refuse_if_final_template_policy_differs(&default_cache, &default_path, false).unwrap();
    refuse_if_final_template_policy_differs(&override_cache, &override_path, true).unwrap();
}

/// A second, structurally different fixture, for the grammar-change/wipe test.
fn secondary_fixture() -> (String, String) {
    fixture_grammar_and_word("edge-cases", "truncate-morphotactic")
}

fn run_batch_args(
    dir: &std::path::Path,
    grammar_xml: &str,
    words_text: &str,
    extra: &[&str],
) -> (Vec<String>, PathBuf) {
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
    args.extend(extra.iter().map(|s| s.to_string()));
    (args, out_path)
}

fn stable_batch_fields(tsv: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for line in tsv.lines() {
        let columns: Vec<_> = line.split('\t').collect();
        if columns.len() == 3 && columns[2] == "STARTED" {
            continue;
        }
        assert!(
            matches!(columns.len(), 5 | 6),
            "batch TSV result row must have five or six columns: {line:?}"
        );
        rows.push(
            columns
                .into_iter()
                .enumerate()
                .filter(|(index, _)| *index != 2)
                .map(|(_, value)| value.to_owned())
                .collect(),
        );
    }
    assert!(
        !rows.is_empty(),
        "batch TSV must contain at least one result row"
    );
    rows
}

fn legacy_stats_words(
    grammar: &Grammar,
    morpher: &pg_parse::Morpher,
    opts: &pg_parse::ParseOptions,
    words: &[String],
) -> Vec<BatchStatsWord> {
    let identities = pg_grammar::stats_identity::StatsIdentityCatalog::new(grammar);
    words
        .iter()
        .map(|word| {
            let start = Instant::now();
            let (outcome, rows, prune_rows) = morpher.parse_word_with_stats_and_prunes(word, opts);
            let result = pg_parse::BatchWordOutcome {
                outcome,
                elapsed: start.elapsed(),
            };
            batch_stats_word(grammar, &identities, word, &result, &rows, &prune_rows)
        })
        .collect()
}

fn stable_cache_snapshot(path: &std::path::Path) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
    let conn = rusqlite::Connection::open(path).expect("open stats cache");
    let query = |sql: &str| {
        let mut statement = conn.prepare(sql).expect("prepare stats snapshot");
        let column_count = statement.column_count();
        statement
            .query_map([], |row| {
                (0..column_count)
                    .map(|index| {
                        row.get::<_, rusqlite::types::Value>(index)
                            .map(|value| format!("{value:?}"))
                    })
                    .collect()
            })
            .expect("query stats snapshot")
            .collect::<Result<Vec<_>, _>>()
            .expect("read stats snapshot")
    };
    let words = query(
        "SELECT form, attempts, passes, capped, timed_out, invalid_shape \
             FROM word ORDER BY form",
    );
    let facts = query(
        "SELECT w.form, o.key, o.kind, o.label, o.identity_quality, m.key, s.key, a.key, \
                    f.direction, f.attempts, f.work, f.outputs, f.not_applied, f.no_root, \
                    f.surface_mismatch, f.uses \
             FROM fact f \
             JOIN word w ON w.word_id = f.word_id \
             JOIN object o ON o.object_id = f.object_id \
             LEFT JOIN morpheme m ON m.morpheme_id = o.morpheme_id \
             LEFT JOIN stratum s ON s.stratum_id = f.stratum_id \
             LEFT JOIN allomorph a ON a.allomorph_id = f.allomorph_id \
             ORDER BY w.form, o.key, o.kind, s.key, a.key, f.direction",
    );
    (words, facts)
}

#[test]
fn batch_stats_produces_nonempty_object_report_and_tsv_results_stay_identical() {
    let (grammar_xml, word) = primary_fixture();
    let words_text = format!("{word}\n");

    let dir_plain = scratch_dir("tsv-plain");
    let (args_plain, out_plain) = run_batch_args(&dir_plain, &grammar_xml, &words_text, &[]);
    crate::run_batch(&args_plain).expect("plain batch run");
    let tsv_plain = fs::read_to_string(&out_plain).expect("read plain tsv");

    let dir_stats = scratch_dir("tsv-stats");
    let cache_path = dir_stats.join("cache.sqlite3");
    let (args_stats, out_stats) = run_batch_args(
        &dir_stats,
        &grammar_xml,
        &words_text,
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&args_stats).expect("stats batch run");
    let tsv_stats = fs::read_to_string(&out_stats).expect("read stats tsv");

    assert_eq!(
        stable_batch_fields(&tsv_plain),
        stable_batch_fields(&tsv_stats),
        "batch's stable TSV fields must be identical with or without --stats; elapsed milliseconds are measured independently"
    );

    let conn = rusqlite::Connection::open(&cache_path).unwrap();
    let rows = pg_stats::per_object_report(&conn, &pg_stats::PerObjectFilter::default()).unwrap();
    assert!(
        !rows.is_empty(),
        "a fixture with morphological rules must produce at least one per-object row"
    );
    let morph_row = rows
        .iter()
        .find(|r| r.kind == "morph_rule")
        .expect("this fixture must produce a morph_rule row");
    assert_eq!(
        cell_value("morph_rule", "attempts", morph_row.attempts),
        Some(morph_row.attempts),
        "the hc engine's attempts counter is Measured for morph_rule"
    );
}

#[test]
fn batch_stats_parses_each_uncached_word_once() {
    let (grammar_xml, word) = primary_fixture();
    let words_text = format!("{word}\n{word}x\n");
    let dir = scratch_dir("single-parse-fire-count");
    let cache_path = dir.join("cache.sqlite3");
    let (args, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &words_text,
        &["--threads", "1", "--cache", cache_path.to_str().unwrap()],
    );

    let plain_fires = crate::run_batch_counted(&args).expect("plain batch run");
    assert_eq!(
        plain_fires, 2,
        "plain batch should enter the Morpher once per word"
    );

    let (stats_args, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &words_text,
        &[
            "--threads",
            "1",
            "--stats",
            "--cache",
            cache_path.to_str().unwrap(),
        ],
    );
    let stats_fires = crate::run_batch_counted(&stats_args).expect("stats batch run");

    assert_eq!(
        stats_fires, 2,
        "each uncached word should enter the Morpher once when --stats is enabled"
    );

    let cached_dir = scratch_dir("single-parse-cached-prefix-seed");
    let cached_path = cached_dir.join("cache.sqlite3");
    let (seed_args, _) = run_batch_args(
        &cached_dir,
        &grammar_xml,
        &format!("{word}\n"),
        &[
            "--threads",
            "1",
            "--stats",
            "--cache",
            cached_path.to_str().unwrap(),
        ],
    );
    crate::run_batch(&seed_args).expect("seed first word into stats cache");

    let remaining_words = format!("{word}\n{word}x\n{word}y\n");
    for threads in ["1", "2"] {
        let dir = scratch_dir(&format!("single-parse-cached-prefix-{threads}-threads"));
        let (args, _) = run_batch_args(
            &dir,
            &grammar_xml,
            &remaining_words,
            &[
                "--threads",
                threads,
                "--start",
                "1",
                "--stats",
                "--cache",
                cached_path.to_str().unwrap(),
            ],
        );
        let fires = crate::run_batch_counted(&args).expect("cached-prefix stats batch run");
        assert_eq!(
            fires,
            2,
            "with --threads {threads}, a cached word before --start must be skipped and each of the two output words must be parsed once"
        );
    }
}

fn cached_run_id(path: &std::path::Path, word: &str) -> i64 {
    rusqlite::Connection::open(path)
        .expect("open stats cache")
        .query_row("SELECT run_id FROM word WHERE form = ?1", [word], |row| {
            row.get(0)
        })
        .expect("word has an owning run")
}

fn run_stats_with_options(
    dir: &std::path::Path,
    grammar_xml: &str,
    words: &[String],
    cache_path: &std::path::Path,
    guess: bool,
    timeout_ms: Option<u64>,
) -> usize {
    let mut extra = vec![
        "--threads".to_string(),
        "1".to_string(),
        "--stats".to_string(),
        "--cache".to_string(),
        cache_path.to_string_lossy().into_owned(),
    ];
    if guess {
        extra.push("--guess".to_string());
    }
    if let Some(timeout_ms) = timeout_ms {
        extra.extend(["--word-timeout-ms".to_string(), timeout_ms.to_string()]);
    }
    let extra_refs = extra.iter().map(String::as_str).collect::<Vec<_>>();
    let words_text = format!("{}\n", words.join("\n"));
    let (args, _) = run_batch_args(dir, grammar_xml, &words_text, &extra_refs);
    crate::run_batch_counted(&args).expect("batch stats run succeeds");
    let analyzed: i64 = rusqlite::Connection::open(cache_path)
        .expect("open stats cache after run")
        .query_row(
            "SELECT word_count FROM run ORDER BY run_id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("stats run records analyzed word count");
    usize::try_from(analyzed).expect("analyzed word count is nonnegative")
}

#[test]
fn batch_stats_reuses_only_words_owned_by_the_current_measurement_options() {
    let (grammar_xml, first_word) = primary_fixture();
    let second_word = format!("{first_word}x");
    let dir = scratch_dir("option-aware-cache-ownership");
    let cache_path = dir.join("cache.sqlite3");
    let unchanged_word = second_word.as_str();

    assert_eq!(
        run_stats_with_options(
            &dir,
            &grammar_xml,
            &[first_word.clone(), second_word.clone()],
            &cache_path,
            false,
            None,
        ),
        2,
        "initial options record both words as analyzed"
    );
    let first_owner = cached_run_id(&cache_path, &first_word);
    let unchanged_owner = cached_run_id(&cache_path, unchanged_word);

    assert_eq!(
        run_stats_with_options(
            &dir,
            &grammar_xml,
            &[first_word.clone(), second_word.clone()],
            &cache_path,
            false,
            None,
        ),
        0,
        "an exact options match reuses both words"
    );
    assert_eq!(cached_run_id(&cache_path, &first_word), first_owner);

    for (guess, timeout_ms, label) in [
        (true, None, "guess on"),
        (false, None, "guess off again"),
        (false, Some(10_000), "timeout added"),
        (false, Some(20_000), "timeout changed"),
    ] {
        assert_eq!(
            run_stats_with_options(
                &dir,
                &grammar_xml,
                std::slice::from_ref(&first_word),
                &cache_path,
                guess,
                timeout_ms,
            ),
            1,
            "changing measurement options must reanalyze the overlapping word ({label})"
        );
        assert_ne!(
            cached_run_id(&cache_path, &first_word),
            first_owner,
            "the changed stats observation must become the word's owner ({label})"
        );
        assert_eq!(
            cached_run_id(&cache_path, unchanged_word),
            unchanged_owner,
            "a different word retains its prior compatible stats owner ({label})"
        );
    }

    let latest_owner = cached_run_id(&cache_path, &first_word);
    assert_eq!(
        run_stats_with_options(
            &dir,
            &grammar_xml,
            std::slice::from_ref(&first_word),
            &cache_path,
            false,
            Some(20_000),
        ),
        0,
        "the latest exact options match reuses its stats record"
    );
    assert_eq!(cached_run_id(&cache_path, &first_word), latest_owner);
}

#[test]
fn batch_stats_preserves_legacy_cache_and_tsv_across_thread_counts_and_options() {
    let (grammar_xml, word) = primary_fixture();
    let words_text = format!("{word}\n{word}x\n{word}y\n");
    let words = vec![word.clone(), format!("{word}x"), format!("{word}y")];
    let step_cap = StepCap::Finite(std::num::NonZeroU64::new(50_000_000).unwrap());
    let timeout_ms = Some(10_000);

    let dir_legacy = scratch_dir("legacy-stats-snapshot");
    let (legacy_args, legacy_tsv_path) = run_batch_args(
        &dir_legacy,
        &grammar_xml,
        &words_text,
        &[
            "--threads",
            "1",
            "--start",
            "1",
            "--guess",
            "--step-cap",
            "50000000",
            "--word-timeout-ms",
            "10000",
            "--always-enforce-final-templates",
        ],
    );
    crate::run_batch(&legacy_args).expect("plain baseline batch run");
    let legacy_tsv = fs::read_to_string(legacy_tsv_path).expect("read plain baseline tsv");
    let legacy_grammar_path = &legacy_args[0];
    let legacy_cache_path = dir_legacy.join("legacy.sqlite3");
    let (grammar, _warnings) = crate::load_grammar(legacy_grammar_path).expect("load grammar");
    let morpher = pg_parse::Morpher::new(&grammar, step_cap.as_morpher_cap())
        .with_word_timeout(timeout_ms.map(Duration::from_millis))
        .with_always_enforce_final_templates(true);
    let opts = pg_parse::ParseOptions::default().with_guess_root(true);
    let expected_words = legacy_stats_words(&grammar, &morpher, &opts, &words);
    run_batch_stats_hc(
        legacy_grammar_path,
        &words,
        expected_words,
        step_cap,
        timeout_ms,
        true,
        true,
        Some(legacy_cache_path.to_str().unwrap()),
    )
    .expect("write legacy second-pass cache");
    let legacy_cache = stable_cache_snapshot(&legacy_cache_path);

    let mut parallel_tsv = None;
    let mut parallel_cache = None;
    for threads in ["1", "3"] {
        let dir = scratch_dir(&format!("single-pass-{threads}-threads"));
        let cache_path = dir.join("cache.sqlite3");
        let (args, tsv_path) = run_batch_args(
            &dir,
            &grammar_xml,
            &words_text,
            &[
                "--threads",
                threads,
                "--start",
                "1",
                "--guess",
                "--step-cap",
                "50000000",
                "--word-timeout-ms",
                "10000",
                "--always-enforce-final-templates",
                "--stats",
                "--cache",
                cache_path.to_str().unwrap(),
            ],
        );
        crate::run_batch(&args).expect("stats batch run");
        let tsv = fs::read_to_string(tsv_path).expect("read stats tsv");
        assert_eq!(
            stable_batch_fields(&legacy_tsv),
            stable_batch_fields(&tsv),
            "stable TSV fields must match the legacy parse with --threads {threads}"
        );
        let cache = stable_cache_snapshot(&cache_path);
        assert_eq!(
            legacy_cache, cache,
            "stable cache dimensions and counters must match the old stats-only pass with --threads {threads}"
        );
        let stable_tsv = stable_batch_fields(&tsv);
        if let Some(previous) = parallel_tsv.replace(stable_tsv.clone()) {
            assert_eq!(
                previous, stable_tsv,
                "thread counts must preserve the same stable TSV rows"
            );
        }
        if let Some(previous) = parallel_cache.replace(cache.clone()) {
            assert_eq!(
                previous, cache,
                "thread counts must produce the same stable stats cache"
            );
        }
    }
}

#[test]
fn batch_stats_cache_refusal_leaves_the_previous_tsv_intact() {
    let (grammar_xml, word) = primary_fixture();
    let dir = scratch_dir("cache-refusal-keeps-tsv");
    let cache_path = dir.join("cache.sqlite3");
    let cache = cache_path.to_str().unwrap();
    let (first, out_path) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--step-cap", "1000", "--cache", cache],
    );
    crate::run_batch(&first).expect("first stats batch run");
    let before = fs::read_to_string(&out_path).expect("read first tsv");
    assert!(!before.is_empty(), "first run must write a TSV row");

    let (second, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--step-cap", "2000", "--cache", cache],
    );
    crate::run_batch(&second).expect_err("a step-cap mismatch must refuse the cache");
    assert_eq!(
        fs::read_to_string(&out_path).expect("read tsv after refusal"),
        before,
        "a refused stats cache must not truncate the previous TSV"
    );
}

#[test]
fn batch_stats_run_twice_skips_already_cached_words() {
    let (grammar_xml, word) = primary_fixture();
    let words_text = format!("{word}\n");
    let dir = scratch_dir("accumulate");
    let cache_path = dir.join("cache.sqlite3");

    let (args_first, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &words_text,
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&args_first).expect("first stats batch run");

    let (args_second, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &words_text,
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&args_second).expect("second stats batch run");

    let conn = rusqlite::Connection::open(&cache_path).unwrap();
    let word_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM word", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        word_count, 1,
        "the same word run twice must accumulate to exactly one word row, not two"
    );
}

/// A grammar whose default final-template prune can actually fire: one ordinary stratum-listed rule, one `final` template whose slot rule is NOT stratum-listed, and no `partial` marker anywhere (any partial rule disables default pruning grammar-wide).
const PRUNE_FIRING_GRAMMAR: &str = r#"<HermitCrabInput><Language><Name>PruneCounterFixture</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cs"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cs" /><Segment segment="ct" /></SegmentNaturalClass></NaturalClasses>
  <Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrOrd">
    <Name>S</Name>
    <MorphologicalRuleDefinitions>
      <MorphologicalRule id="mrOrd" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>ordSuffix</Name>
        <MorphologicalSubrules><MorphologicalSubrule id="subOrd">
          <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrTpl" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>tplSuffix</Name>
        <MorphologicalSubrules><MorphologicalSubrule id="subTpl">
          <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>t</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    </MorphologicalRuleDefinitions>
    <AffixTemplates>
      <AffixTemplate requiredPartsOfSpeech="posV" final="true"><Name>tpl</Name>
        <Slot optional="true" morphologicalRules="mrTpl"><Name>tplSlot</Name></Slot>
      </AffixTemplate>
    </AffixTemplates>
    <LexicalEntries><LexicalEntry id="e1" partOfSpeech="posV"><Allomorphs>
      <Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph>
    </Allomorphs></LexicalEntry></LexicalEntries>
  </Stratum></Strata>
</Language></HermitCrabInput>"#;

/// Drives `run_batch_stats_hc` twice against the same cache; the first run must measure real pruning and the second must read zero, so "newly analyzed" is proven to be the line's actual scope rather than a counter that is always zero.
#[test]
fn run_batch_stats_hc_second_run_on_fully_cached_words_reports_zero_prune_counters() {
    // A hand-built grammar, not a conformance fixture: no fixture in either root satisfies all three preconditions at once, so a fixture-based first run measures zero and cannot tell the two claims apart.
    let (grammar_xml, word) = (PRUNE_FIRING_GRAMMAR.to_string(), "ats".to_string());
    let dir = scratch_dir("prune-counters-newly-analyzed");
    let grammar_path = dir.join("grammar.xml");
    fs::write(&grammar_path, &grammar_xml).expect("write grammar");
    let grammar_path_str = grammar_path.to_str().unwrap().to_string();
    let cache_path = dir.join("cache.sqlite3");
    let cache_path_str = cache_path.to_str().unwrap().to_string();

    let (grammar, _warnings) = crate::load_grammar(&grammar_path_str).expect("load grammar");
    let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
    let opts = pg_parse::ParseOptions::default();
    let words = vec![word];

    let first_stats = legacy_stats_words(&grammar, &morpher, &opts, &words);
    let first = run_batch_stats_hc(
        &grammar_path_str,
        &words,
        first_stats,
        StepCap::Unbounded,
        None,
        false,
        false,
        Some(cache_path_str.as_str()),
    )
    .expect("first stats run");
    assert_ne!(
        first,
        pg_rules::stats::PruneCounters::default(),
        "the first run must measure real pruning, or the second run's zero proves nothing about \
             scope; measured {first:?}"
    );

    let second_stats = legacy_stats_words(&grammar, &morpher, &opts, &words);
    let second = run_batch_stats_hc(
        &grammar_path_str,
        &words,
        second_stats,
        StepCap::Unbounded,
        None,
        false,
        false,
        Some(cache_path_str.as_str()),
    )
    .expect("second stats run");
    assert_eq!(
        second,
        pg_rules::stats::PruneCounters::default(),
        "every word was already cached, so counters must read zero, not the accumulated first-run total"
    );
}

#[test]
fn stats_cache_rejects_switching_final_template_policy() {
    let (grammar_xml, word) = primary_fixture();
    let dir = scratch_dir("policy-switch");
    let cache_path = dir.join("cache.sqlite3");
    let (first, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&first).expect("initial stats run");
    let (second, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &[
            "--stats",
            "--always-enforce-final-templates",
            "--cache",
            cache_path.to_str().unwrap(),
        ],
    );
    let err = crate::run_batch(&second).expect_err("policy switch must be rejected");
    assert!(err.contains(cache_path.to_str().unwrap()), "error: {err}");
    assert!(
        err.contains("false") && err.contains("true"),
        "error: {err}"
    );
}

#[test]
fn batch_stats_wipes_cache_and_reports_it_when_grammar_changes() {
    let (grammar_xml, word) = primary_fixture();
    let (other_grammar_xml, other_word) = secondary_fixture();
    let dir = scratch_dir("wipe");
    let cache_path = dir.join("cache.sqlite3");

    let (args_first, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&args_first).expect("first stats batch run");

    let conn = rusqlite::Connection::open(&cache_path).unwrap();
    let first_word_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM word", [], |row| row.get(0))
        .unwrap();
    assert_eq!(first_word_count, 1);
    drop(conn);

    let (args_second, _) = run_batch_args(
        &dir,
        &other_grammar_xml,
        &format!("{other_word}\n"),
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&args_second).expect("second stats batch run after grammar change");

    let conn = rusqlite::Connection::open(&cache_path).unwrap();
    let second_word_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM word", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        second_word_count, 1,
        "a grammar change must wipe the old word before recording the new one"
    );
    let form: String = conn
        .query_row("SELECT form FROM word", [], |row| row.get(0))
        .unwrap();
    assert_eq!(form, other_word, "only the new grammar's word must remain");
}

fn sample_object_row(no_root: i64) -> pg_stats::PerObjectRow {
    pg_stats::PerObjectRow {
        kind: "morph_rule".to_string(),
        label: "Rule A".to_string(),
        identity_quality: "authored".to_string(),
        attempts: 5,
        work: 20,
        outputs: 2,
        not_applied: 1,
        no_root,
        surface_mismatch: 0,
        uses: 1,
        self_time_ns: 150,
    }
}

/// A genuinely `Measured` counter on the same row must still render as a plain number.
#[test]
fn not_applicable_counter_renders_em_dash_not_zero() {
    // morph_rule carries no surface_mismatch identity of its own (see counter_support's doc).
    let row = sample_object_row(7);
    let view = object_row_view(&row);
    let (headers, table_rows) = render_narrow_self_totals(std::slice::from_ref(&view), true);
    let surface_mismatch_col = headers
        .iter()
        .position(|h| *h == "surface_mismatch")
        .unwrap();
    assert_eq!(
        table_rows[0][surface_mismatch_col], "-",
        "morph_rule's surface_mismatch is NotApplicable -- it must never render as a bare number"
    );

    let no_root_col = headers.iter().position(|h| *h == "no_root").unwrap();
    assert_eq!(
        table_rows[0][no_root_col], "7",
        "falsifiability check: morph_rule's no_root IS measured and must still render as a plain number"
    );
}

#[test]
fn wide_appends_extra_columns_and_is_absent_by_default() {
    let row = sample_object_row(0);
    let view = object_row_view(&row);

    let (narrow_headers, narrow_rows) =
        render_narrow_self_totals(std::slice::from_ref(&view), false);
    assert!(!narrow_headers.contains(&"work"));
    assert!(!narrow_headers.contains(&"identity_quality"));
    assert_eq!(narrow_rows[0].len(), narrow_headers.len());

    let (wide_headers, wide_rows) = render_narrow_self_totals(std::slice::from_ref(&view), true);
    assert!(wide_headers.contains(&"work"));
    assert!(wide_headers.contains(&"not_applied"));
    assert!(wide_headers.contains(&"no_root"));
    assert!(wide_headers.contains(&"surface_mismatch"));
    assert!(wide_headers.contains(&"identity_quality"));
    assert_eq!(wide_rows[0].len(), wide_headers.len());
}

#[test]
fn hc_cache_renders_with_the_columns_foma_would_omit() {
    let (grammar_xml, word) = primary_fixture();
    let dir = scratch_dir("hc-has-columns");
    let cache_path = dir.join("cache.sqlite3");
    let (args, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&args).expect("seed the cache via batch --stats");

    let conn = rusqlite::Connection::open(&cache_path).unwrap();
    let body = render_object(
        &conn,
        &Filters {
            wide: true,
            ..Filters::default()
        },
        OutputFormat::Text,
    )
    .unwrap();
    assert!(
        !body.contains("never records"),
        "an hc cache must never print the foma omission note: {body}"
    );
    let header_line = body
        .lines()
        .find(|l| l.contains("label"))
        .expect("the rendered table must have a header line");
    assert!(
        header_line.contains("attempts"),
        "an hc cache must keep the attempts column: {header_line}"
    );
}

#[test]
fn shares_sum_to_100_pct_per_orientation() {
    let (grammar_xml, word) = primary_fixture();
    let dir = scratch_dir("shares");
    let cache_path = dir.join("cache.sqlite3");
    let (args, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&args).expect("seed the cache via batch --stats");

    let conn = rusqlite::Connection::open(&cache_path).unwrap();
    let rows = pg_stats::per_object_report(&conn, &pg_stats::PerObjectFilter::default()).unwrap();
    assert!(!rows.is_empty(), "sanity: the fixture must produce rows");
    let views: Vec<RowView> = rows.iter().map(object_row_view).collect();
    let (headers, table_rows) = render_narrow_self_totals(&views, false);

    let time_col = headers.iter().position(|h| *h == "time%").unwrap();
    let attempts_col = headers.iter().position(|h| *h == "attempts%").unwrap();
    let pct_of = |row: &[String], col: usize| -> Option<f64> {
        row[col].trim_end_matches('%').parse::<f64>().ok()
    };

    // Self time means the same thing whatever kind produced it, so its shares span the report.
    let time_sum: f64 = table_rows.iter().filter_map(|r| pct_of(r, time_col)).sum();
    assert!(
        (99.0..=101.0).contains(&time_sum),
        "time% must sum to ~100% with no --top narrowing: got {time_sum}"
    );

    // `attempts` does not: each kind counts a different event, so shares close within a kind.
    let mut by_kind: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for (view, row) in views.iter().zip(table_rows.iter()) {
        if let Some(p) = pct_of(row, attempts_col) {
            *by_kind
                .entry(view.kind.clone().unwrap_or_else(|| "-".to_string()))
                .or_insert(0.0) += p;
        }
    }
    assert!(
        by_kind.len() > 1,
        "sanity: this fixture must span several kinds, or the per-kind claim is untested"
    );
    for (kind, sum) in &by_kind {
        assert!(
            (99.0..=101.0).contains(sum),
            "attempts% must sum to ~100% within {kind}, not across kinds: got {sum}"
        );
    }
}

#[test]
fn totals_line_attribution_matches_hand_summed_rows() {
    let (grammar_xml, word) = primary_fixture();
    let dir = scratch_dir("attribution");
    let cache_path = dir.join("cache.sqlite3");
    let (args, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&args).expect("seed the cache via batch --stats");

    let conn = rusqlite::Connection::open(&cache_path).unwrap();
    let body = render_object(&conn, &Filters::default(), OutputFormat::Text).unwrap();
    let total_line = body
        .lines()
        .find(|l| l.starts_with("TOTAL"))
        .expect("a TOTAL line must be printed");

    let rows = pg_stats::per_object_report(&conn, &pg_stats::PerObjectFilter::default()).unwrap();
    let hand_summed_time_ns: i64 = rows.iter().map(|r| r.self_time_ns).sum();
    let run_elapsed_ns = pg_stats::word_elapsed_ns_total(&conn, None).unwrap();
    let expected_pct = if run_elapsed_ns > 0 {
        hand_summed_time_ns as f64 / run_elapsed_ns as f64 * 100.0
    } else {
        0.0
    };
    assert!(
        total_line.contains(&format!("{:.3}ms", hand_summed_time_ns as f64 / 1e6)),
        "TOTAL line must report the exact hand-summed row time: {total_line}"
    );
    assert!(
        total_line.contains(&format!("{expected_pct:.1}% attributed")),
        "TOTAL line's attribution percentage must match summing rows by hand: {total_line}"
    );
    assert!(
        total_line.contains("comparison elapsed across cached words"),
        "{total_line}"
    );

    let json = render_object(&conn, &Filters::default(), OutputFormat::Jsonl).unwrap();
    let meta: serde_json::Value = serde_json::from_str(json.lines().next().unwrap()).unwrap();
    assert_eq!(
        meta["totals"]["comparisonElapsedNs"], run_elapsed_ns,
        "the default comparison denominator is all cached word elapsed time"
    );
    assert_eq!(meta["totals"]["comparisonScope"], "cached_words");
}

#[test]
fn object_comparison_elapsed_uses_word_filter_but_not_censor_or_top_filters() {
    let cache = synthetic_stats_cache_with_words(
        "hc",
        vec![
            pg_stats::WordRecord {
                form: "ok".to_string(),
                elapsed_ns: 1_000,
                attempts: 1,
                passes: 1,
                capped: false,
                timed_out: false,
                invalid_shape: false,
                facts: vec![synthetic_fact(pg_stats::ObjectKind::MorphRule, None, 1)],
            },
            pg_stats::WordRecord {
                form: "capped".to_string(),
                elapsed_ns: 3_000,
                attempts: 1,
                passes: 0,
                capped: true,
                timed_out: false,
                invalid_shape: false,
                facts: vec![synthetic_fact(pg_stats::ObjectKind::MorphRule, None, 1)],
            },
        ],
    );

    let cache_scope = render_object(
        cache.connection(),
        &Filters {
            top_n: Some(0),
            exclude_censored: true,
            ..Filters::default()
        },
        OutputFormat::Jsonl,
    )
    .unwrap();
    let cache_meta: serde_json::Value =
        serde_json::from_str(cache_scope.lines().next().unwrap()).unwrap();
    assert_eq!(cache_meta["totals"]["comparisonElapsedNs"], 4_000);
    assert_eq!(cache_meta["totals"]["comparisonScope"], "cached_words");

    let word_scope = render_object(
        cache.connection(),
        &Filters {
            word: Some("ok".to_string()),
            ..Filters::default()
        },
        OutputFormat::Jsonl,
    )
    .unwrap();
    let word_meta: serde_json::Value =
        serde_json::from_str(word_scope.lines().next().unwrap()).unwrap();
    assert_eq!(word_meta["totals"]["comparisonElapsedNs"], 1_000);
    assert_eq!(word_meta["totals"]["comparisonScope"], "selected_word");
}

#[test]
fn word_filter_narrows_and_absent_word_explains_itself() {
    let (grammar_xml, word) = primary_fixture();
    let dir = scratch_dir("word-filter");
    let cache_path = dir.join("cache.sqlite3");
    let (args, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&args).expect("seed the cache via batch --stats");

    let conn = rusqlite::Connection::open(&cache_path).unwrap();

    let matching = Filters {
        word: Some(word.clone()),
        ..Filters::default()
    };
    let body = render_object(&conn, &matching, OutputFormat::Text).unwrap();
    assert!(
        body.contains("TOTAL"),
        "the word that was actually analyzed must still produce rows: {body}"
    );

    let absent = Filters {
        word: Some("this-word-was-never-analyzed-xyz".to_string()),
        ..Filters::default()
    };
    let empty_body = render_object(&conn, &absent, OutputFormat::Text).unwrap();
    assert!(
        empty_body.contains("objects exist in this cache, but none match this filter"),
        "a real kind narrowed to an absent word must explain the empty result: {empty_body}"
    );
}

#[test]
fn top_n_is_scoped_per_kind_and_never_narrows_the_totals() {
    let (grammar_xml, word) = primary_fixture();
    let dir = scratch_dir("top-per-kind");
    let cache_path = dir.join("cache.sqlite3");
    let (args, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&args).expect("seed the cache via batch --stats");

    let conn = rusqlite::Connection::open(&cache_path).unwrap();
    let all_rows =
        pg_stats::per_object_report(&conn, &pg_stats::PerObjectFilter::default()).unwrap();
    let kinds: std::collections::HashSet<_> = all_rows.iter().map(|r| r.kind.clone()).collect();
    assert!(
        all_rows.len() > kinds.len(),
        "this fixture must record more objects than kinds, or --top drops nothing to test"
    );

    let untopped = render_object(&conn, &Filters::default(), OutputFormat::Text).unwrap();
    let topped = render_object(
        &conn,
        &Filters {
            top_n: Some(1),
            ..Filters::default()
        },
        OutputFormat::Text,
    )
    .unwrap();

    for kind in &kinds {
        let shown = topped
            .lines()
            .filter(|l| l.starts_with(&format!("{kind}: ")))
            .count();
        assert_eq!(
            shown, 1,
            "--top 1 must keep exactly one row of kind {kind}, never collapse to a single \
                 global winner: {topped}"
        );
    }

    let total_time_field = |body: &str| {
        body.lines()
            .find(|l| l.starts_with("TOTAL"))
            .expect("every rendered table carries a TOTAL line")
            .split("   ")
            .find(|f| f.trim_start().starts_with("time "))
            .expect("the TOTAL line carries a time field")
            .trim()
            .to_string()
    };
    assert_eq!(
        total_time_field(&topped),
        total_time_field(&untopped),
        "a --top excerpt must total (and attribute) every matched row, not just shown ones"
    );
    let total_line = topped.lines().find(|l| l.starts_with("TOTAL")).unwrap();
    assert!(
        total_line.contains(&format!(
            "{} row(s) ({} shown)",
            all_rows.len(),
            kinds.len()
        )),
        "a truncated table must say how many of the matched rows it displayed: {total_line}"
    );
}

#[test]
fn a_shown_row_states_its_share_of_every_matched_row_not_of_the_excerpt() {
    let row = RowView {
        label: "r".to_string(),
        kind: Some("morph_rule".to_string()),
        identity_quality: None,
        self_time_ns: Some(1_000),
        attempts: Some(10),
        outputs: Some(0),
        uses: Some(0),
        work: Some(0),
        not_applied: Some(0),
        no_root: Some(0),
        surface_mismatch: Some(0),
    };
    // One row displayed out of a matched set totalling four times its time and attempts.
    let denoms = Denominators {
        total_time_ns: Some(4_000),
        attempts_by_kind: HashMap::from([(Some("morph_rule".to_string()), Some(40))]),
    };
    let (headers, rows) = render_narrow(&[row], false, &denoms, &[]);
    let time_pct = headers.iter().position(|h| *h == "time%").unwrap();
    let attempts_pct = headers.iter().position(|h| *h == "attempts%").unwrap();
    assert_eq!(
        (rows[0][time_pct].as_str(), rows[0][attempts_pct].as_str()),
        ("25.0%", "25.0%"),
        "a lone displayed row must not read as 100% of the run"
    );
}

#[test]
fn jsonl_first_line_is_meta_and_every_row_parses() {
    let (grammar_xml, word) = primary_fixture();
    let dir = scratch_dir("jsonl");
    let cache_path = dir.join("cache.sqlite3");
    let (args, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&args).expect("seed the cache via batch --stats");

    let conn = rusqlite::Connection::open(&cache_path).unwrap();
    let body = render_object(&conn, &Filters::default(), OutputFormat::Jsonl).unwrap();
    let mut lines = body.lines();

    let meta_line = lines.next().expect("jsonl output must have a meta line");
    let meta: serde_json::Value =
        serde_json::from_str(meta_line).expect("meta line must be valid JSON");
    assert_eq!(meta["meta"], serde_json::Value::Bool(true));
    assert_eq!(meta["orientation"], "object");

    let mut row_count = 0;
    for line in lines {
        let _: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("row line failed to parse: {line}: {e}"));
        row_count += 1;
    }
    assert!(
        row_count > 0,
        "at least one data row must follow the meta line"
    );
}

#[test]
fn empty_result_distinguishes_never_recorded_from_filtered_to_nothing() {
    let dir = scratch_dir("empty-explain");
    let cache_path = dir.join("cache.sqlite3");
    let mut outcome = pg_stats::StatsCache::open(&cache_path, "hash-a").unwrap();
    let run = pg_stats::RunMetadata {
        build_info: "test".to_string(),
        fwdata_path: "x".to_string(),
        grammar_hash: "hash-a".to_string(),
        engine: "hc".to_string(),
        options_hash: "opts".to_string(),
        options_json: "{}".to_string(),
        created_utc: "unix:0".to_string(),
        step_cap: None,
    };
    let word_record = pg_stats::WordRecord {
        form: "onlyword".to_string(),
        elapsed_ns: 1_000,
        attempts: 1,
        passes: 1,
        capped: false,
        timed_out: false,
        invalid_shape: false,
        facts: vec![pg_stats::FactRecord {
            object_key: "rule-a".to_string(),
            object_kind: pg_stats::ObjectKind::MorphRule,
            object_label: "Rule A".to_string(),
            identity_quality: pg_stats::IdentityQuality::Authored,
            stratum: Some(pg_stats::StructuralLocator::new("0:Root", "Root")),
            allomorph: None,
            morpheme: None,
            direction: pg_stats::Direction::Analysis,
            attempts: 5,
            work: 10,
            outputs: 2,
            not_applied: 1,
            no_root: 0,
            surface_mismatch: 0,
            uses: 1,
            self_time_ns: 100,
        }],
    };
    outcome.cache.flush(&run, &[word_record]).unwrap();

    let conn = outcome.cache.connection();

    let never_recorded = Filters {
        kind: Some("phon_rule".to_string()),
        ..Filters::default()
    };
    let body_a = render_object(conn, &never_recorded, OutputFormat::Text).unwrap();
    assert!(
        body_a.contains("no phon_rule objects have ever been recorded"),
        "must explain that this kind never occurred at all: {body_a}"
    );

    let filtered_to_nothing = Filters {
        kind: Some("morph_rule".to_string()),
        word: Some("a-word-never-analyzed".to_string()),
        ..Filters::default()
    };
    let body_b = render_object(conn, &filtered_to_nothing, OutputFormat::Text).unwrap();
    assert!(
        body_b.contains("morph_rule objects exist in this cache, but none match this filter"),
        "must explain that this kind exists but the filter matched nothing: {body_b}"
    );
}

#[test]
fn group_orientation_reports_one_row_per_kind() {
    let (grammar_xml, word) = primary_fixture();
    let dir = scratch_dir("group-orientation");
    let cache_path = dir.join("cache.sqlite3");
    let (args, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&args).expect("seed the cache via batch --stats");

    let conn = rusqlite::Connection::open(&cache_path).unwrap();
    let body = render_group(&conn, &Filters::default(), OutputFormat::Text).unwrap();
    assert!(
        body.contains("morph_rule"),
        "group orientation must surface the morph_rule kind: {body}"
    );
}

#[test]
fn morpheme_orientation_collapses_scattered_entries_through_the_cli() {
    let dir = scratch_dir("morpheme-cli");
    let cache_path = dir.join("cache.sqlite3");
    let mut outcome = pg_stats::StatsCache::open(&cache_path, "hash-a").unwrap();
    let run = pg_stats::RunMetadata {
        build_info: "test".to_string(),
        fwdata_path: "x".to_string(),
        grammar_hash: "hash-a".to_string(),
        engine: "hc".to_string(),
        options_hash: "opts".to_string(),
        options_json: "{}".to_string(),
        created_utc: "unix:0".to_string(),
        step_cap: None,
    };
    let make_fact = |key: &str, morpheme_key: &str, attempts: u64| pg_stats::FactRecord {
        object_key: key.to_string(),
        object_kind: pg_stats::ObjectKind::LexEntry,
        object_label: key.to_string(),
        identity_quality: pg_stats::IdentityQuality::Authored,
        stratum: Some(pg_stats::StructuralLocator::new("0:Root", "Root")),
        allomorph: None,
        morpheme: Some(pg_stats::StructuralLocator::new(morpheme_key, morpheme_key)),
        direction: pg_stats::Direction::Analysis,
        attempts,
        work: attempts,
        outputs: attempts,
        not_applied: 0,
        no_root: 0,
        surface_mismatch: 0,
        uses: 0,
        self_time_ns: attempts * 10,
    };
    let word_record = pg_stats::WordRecord {
        form: "w".to_string(),
        elapsed_ns: 1_000,
        attempts: 1,
        passes: 1,
        capped: false,
        timed_out: false,
        invalid_shape: false,
        facts: vec![
            make_fact("entry-1", "cat-morph", 3),
            make_fact("entry-2", "cat-morph", 2),
        ],
    };
    outcome.cache.flush(&run, &[word_record]).unwrap();

    let conn = outcome.cache.connection();
    let body = render_morpheme(conn, &Filters::default(), OutputFormat::Text).unwrap();
    let data_rows = body.lines().filter(|l| l.contains("cat-morph")).count();
    assert_eq!(
        data_rows, 1,
        "two entries sharing a morpheme must collapse to one row: {body}"
    );
}

#[test]
fn run_stats_end_to_end_group_object_writes_jsonl_and_default_view_succeeds() {
    let (grammar_xml, word) = primary_fixture();
    let dir = scratch_dir("run-stats-e2e");
    let cache_path = dir.join("cache.sqlite3");
    let (batch_args, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&batch_args).expect("seed the cache via batch --stats");

    let grammar_path = dir.join("grammar.xml");
    let jsonl_path = dir.join("object.jsonl");
    let stats_args: Vec<String> = vec![
        grammar_path.to_string_lossy().into_owned(),
        "--group".to_string(),
        "object".to_string(),
        "--cache".to_string(),
        cache_path.to_string_lossy().into_owned(),
        "--format".to_string(),
        "jsonl".to_string(),
        "--out".to_string(),
        jsonl_path.to_string_lossy().into_owned(),
    ];
    run_stats(&stats_args).expect("run_stats --group object --format jsonl --out must succeed");
    let jsonl_text = fs::read_to_string(&jsonl_path).unwrap();
    let mut lines = jsonl_text.lines();
    let meta: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(meta["meta"], serde_json::Value::Bool(true));
    let mut n = 0;
    for line in lines {
        serde_json::from_str::<serde_json::Value>(line).unwrap();
        n += 1;
    }
    assert!(n > 0);

    let default_args: Vec<String> = vec![
        grammar_path.to_string_lossy().into_owned(),
        "--cache".to_string(),
        cache_path.to_string_lossy().into_owned(),
    ];
    run_stats(&default_args).expect("run_stats with no --group must render the default view");
}

#[test]
fn jsonl_format_requires_group() {
    let (grammar_xml, word) = primary_fixture();
    let dir = scratch_dir("jsonl-requires-group");
    let cache_path = dir.join("cache.sqlite3");
    let (batch_args, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--cache", cache_path.to_str().unwrap()],
    );
    crate::run_batch(&batch_args).expect("seed the cache via batch --stats");

    let grammar_path = dir.join("grammar.xml");
    let args: Vec<String> = vec![
        grammar_path.to_string_lossy().into_owned(),
        "--cache".to_string(),
        cache_path.to_string_lossy().into_owned(),
        "--format".to_string(),
        "jsonl".to_string(),
    ];
    let err = run_stats(&args).expect_err("--format jsonl with no --group must be refused");
    assert!(err.contains("--format jsonl requires --group"));
}

#[test]
fn public_stats_groups_do_not_expose_internal_stratum_or_direction_orientations() {
    assert!(!STATS_USAGE.contains("|stratum"));
    assert!(!STATS_USAGE.contains("|direction"));
    for removed in ["stratum", "direction"] {
        let args = vec![
            "unused.xml".to_string(),
            "--group".to_string(),
            removed.to_string(),
        ];
        let err = run_stats(&args).expect_err("removed report groups must be rejected");
        assert!(err.contains("invalid --group"), "{removed}: {err}");
    }
}

fn synthetic_stats_cache(engine: &str, facts: Vec<pg_stats::FactRecord>) -> pg_stats::StatsCache {
    synthetic_stats_cache_with_words(
        engine,
        vec![pg_stats::WordRecord {
            form: "w".to_string(),
            elapsed_ns: 1000,
            attempts: 4,
            passes: 1,
            capped: false,
            timed_out: false,
            invalid_shape: false,
            facts,
        }],
    )
}

fn synthetic_stats_cache_with_words(
    engine: &str,
    words: Vec<pg_stats::WordRecord>,
) -> pg_stats::StatsCache {
    let dir = scratch_dir("contract");
    let path = dir.join("cache.sqlite3");
    let mut outcome = pg_stats::StatsCache::open(&path, "contract-hash").unwrap();
    let run = pg_stats::RunMetadata {
        build_info: "test".to_string(),
        fwdata_path: "x".to_string(),
        grammar_hash: "contract-hash".to_string(),
        engine: engine.to_string(),
        options_hash: "opts".to_string(),
        options_json: "{}".to_string(),
        created_utc: "unix:0".to_string(),
        step_cap: None,
    };
    outcome.cache.flush(&run, &words).unwrap();
    outcome.cache
}

fn synthetic_fact(
    kind: pg_stats::ObjectKind,
    allomorph: Option<(&str, &str)>,
    attempts: u64,
) -> pg_stats::FactRecord {
    pg_stats::FactRecord {
        object_key: "rule-a".to_string(),
        object_kind: kind,
        object_label: "Rule A".to_string(),
        identity_quality: pg_stats::IdentityQuality::Authored,
        stratum: Some(pg_stats::StructuralLocator::new("0:Root", "Root")),
        allomorph: allomorph.map(|(k, l)| pg_stats::StructuralLocator::new(k, l)),
        morpheme: None,
        direction: pg_stats::Direction::Analysis,
        attempts,
        work: 10,
        outputs: 2,
        not_applied: 1,
        no_root: 0,
        surface_mismatch: 0,
        uses: 1,
        self_time_ns: 100,
    }
}

#[test]
fn named_allomorph_report_does_not_claim_rule_attempts() {
    let cache = synthetic_stats_cache(
        "hc",
        vec![
            synthetic_fact(pg_stats::ObjectKind::MorphRule, None, 4),
            synthetic_fact(
                pg_stats::ObjectKind::MorphRule,
                Some(("rule-a:0", "Allo A")),
                4,
            ),
        ],
    );
    let text =
        render_allomorph(cache.connection(), &Filters::default(), OutputFormat::Text).unwrap();
    let header = text.lines().find(|line| line.contains("label")).unwrap();
    assert!(header.contains("attempts"), "{text}");
    assert!(header.contains("amp"), "{text}");

    let json =
        render_allomorph(cache.connection(), &Filters::default(), OutputFormat::Jsonl).unwrap();
    let named = json
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|row| row["label"].as_str().is_some_and(|s| s.contains("Allo A")))
        .unwrap();
    assert!(named["attempts"].is_null(), "{named}");
    assert!(named["amp"].is_null(), "{named}");
    assert!(named["uses"].is_null(), "{named}");
    assert!(named["no_root"].is_null(), "{named}");
    assert_eq!(named["outputs"], 2);
    let meta: serde_json::Value = serde_json::from_str(json.lines().next().unwrap()).unwrap();
    assert_eq!(
        meta["unmeasured"]["attempts"],
        "MorphRule named-allomorph rows have rule-level attempts only"
    );
    assert_eq!(
        meta["unmeasured"]["uses"],
        "MorphRule named-allomorph rows have rule-level uses only"
    );
    assert_eq!(
        meta["unmeasured"]["no_root"],
        "MorphRule named-allomorph rows have rule-level no_root only"
    );

    let grouped_json = render_allomorph(
        cache.connection(),
        &Filters {
            by_kind: true,
            ..Filters::default()
        },
        OutputFormat::Jsonl,
    )
    .unwrap();
    let subtotal = grouped_json
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|row| row["subtotal"] == true)
        .unwrap();
    assert!(subtotal["attempts"].is_null(), "{subtotal}");
}

#[test]
fn foma_word_report_does_not_claim_unmeasured_attempts() {
    let cache = synthetic_stats_cache("foma", vec![]);
    let text = render_word(cache.connection(), &Filters::default(), OutputFormat::Text).unwrap();
    let header = text.lines().find(|line| line.contains("form")).unwrap();
    assert!(!header.contains("attempts"), "{text}");

    let json = render_word(cache.connection(), &Filters::default(), OutputFormat::Jsonl).unwrap();
    let meta: serde_json::Value = serde_json::from_str(json.lines().next().unwrap()).unwrap();
    assert_eq!(
        meta["unmeasured"]["attempts"],
        "engine=foma never records it"
    );
    let row: serde_json::Value = serde_json::from_str(json.lines().nth(1).unwrap()).unwrap();
    assert!(row["attempts"].is_null(), "{row}");
}

#[test]
fn word_report_exports_invalid_shape_in_text_and_json() {
    let cache = synthetic_stats_cache_with_words(
        "hc",
        vec![pg_stats::WordRecord {
            form: "bad-shape".to_string(),
            elapsed_ns: 500,
            attempts: 4,
            passes: 0,
            capped: false,
            timed_out: false,
            invalid_shape: true,
            facts: vec![],
        }],
    );

    let text = render_word(cache.connection(), &Filters::default(), OutputFormat::Text).unwrap();
    assert!(text.contains("invalid_shape"), "{text}");
    assert!(text.contains("bad-shape"), "{text}");
    assert!(text.contains("true"), "{text}");

    let json = render_word(cache.connection(), &Filters::default(), OutputFormat::Jsonl).unwrap();
    let row: serde_json::Value = serde_json::from_str(json.lines().nth(1).unwrap()).unwrap();
    assert_eq!(row["invalid_shape"], true, "{row}");
}

#[test]
fn word_report_applies_censor_and_top_filters_to_rows() {
    let cache = synthetic_stats_cache_with_words(
        "hc",
        vec![
            pg_stats::WordRecord {
                form: "ok".to_string(),
                elapsed_ns: 100,
                attempts: 1,
                passes: 1,
                capped: false,
                timed_out: false,
                invalid_shape: false,
                facts: vec![],
            },
            pg_stats::WordRecord {
                form: "capped".to_string(),
                elapsed_ns: 500,
                attempts: 1,
                passes: 0,
                capped: true,
                timed_out: false,
                invalid_shape: false,
                facts: vec![],
            },
            pg_stats::WordRecord {
                form: "timed-out".to_string(),
                elapsed_ns: 400,
                attempts: 1,
                passes: 0,
                capped: false,
                timed_out: true,
                invalid_shape: false,
                facts: vec![],
            },
        ],
    );

    let censored = render_word(
        cache.connection(),
        &Filters {
            exclude_censored: true,
            ..Filters::default()
        },
        OutputFormat::Jsonl,
    )
    .unwrap();
    let censored_rows: Vec<serde_json::Value> = censored
        .lines()
        .skip(1)
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(censored_rows.len(), 1, "{censored}");
    assert_eq!(censored_rows[0]["form"], "ok");

    let top = render_word(
        cache.connection(),
        &Filters {
            top_n: Some(1),
            ..Filters::default()
        },
        OutputFormat::Jsonl,
    )
    .unwrap();
    let top_rows: Vec<serde_json::Value> = top
        .lines()
        .skip(1)
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(top_rows.len(), 1, "{top}");
    assert_eq!(top_rows[0]["form"], "capped");
    let meta: serde_json::Value = serde_json::from_str(top.lines().next().unwrap()).unwrap();
    assert_eq!(meta["filters"]["top"], 1);
    assert_eq!(meta["totals"]["rows"], 3, "totals retain all matches");
    assert_eq!(meta["totals"]["rows_shown"], 1, "{meta}");
}

#[test]
fn newly_timed_object_kinds_render_self_time() {
    for kind in ["guesser", "overlay"] {
        let row = pg_stats::PerObjectRow {
            kind: kind.to_string(),
            label: kind.to_string(),
            identity_quality: "synthetic".to_string(),
            attempts: 1,
            work: 1,
            outputs: 0,
            not_applied: 0,
            no_root: 0,
            surface_mismatch: 0,
            uses: 0,
            self_time_ns: 0,
        };
        assert_eq!(object_row_view(&row).self_time_ns, Some(0), "{kind}");
    }
}

#[test]
fn timed_object_kinds_render_their_self_time() {
    for kind in [
        "morph_rule",
        "phon_rule",
        "lex_entry",
        "root_index",
        "guesser",
        "overlay",
    ] {
        let row = pg_stats::PerObjectRow {
            kind: kind.to_string(),
            label: kind.to_string(),
            identity_quality: "synthetic".to_string(),
            attempts: 1,
            work: 1,
            outputs: 0,
            not_applied: 0,
            no_root: 0,
            surface_mismatch: 0,
            uses: 0,
            self_time_ns: 7,
        };
        assert_eq!(object_row_view(&row).self_time_ns, Some(7), "{kind}");
    }
}

#[test]
fn direction_filter_does_not_claim_unmeasured_lexical_synthesis_time() {
    let row = pg_stats::PerObjectRow {
        kind: "lex_entry".to_string(),
        label: "root".to_string(),
        identity_quality: "synthetic".to_string(),
        attempts: 0,
        work: 0,
        outputs: 0,
        not_applied: 0,
        no_root: 0,
        surface_mismatch: 1,
        uses: 0,
        self_time_ns: 0,
    };
    assert_eq!(
        direction_time_view(object_row_view(&row), Some("synthesis")).self_time_ns,
        None
    );
    assert_eq!(
        direction_time_view(object_row_view(&row), Some("analysis")).self_time_ns,
        Some(0)
    );
}

#[test]
fn newly_timed_kind_totals_include_measured_time() {
    let cache = synthetic_stats_cache(
        "hc",
        vec![synthetic_fact(pg_stats::ObjectKind::Guesser, None, 4)],
    );
    let filters = Filters {
        kind: Some("guesser".to_string()),
        ..Filters::default()
    };
    let text = render_object(cache.connection(), &filters, OutputFormat::Text).unwrap();
    let total = text.lines().find(|line| line.starts_with("TOTAL")).unwrap();
    assert!(!total.contains("time -"), "{total}");

    let json = render_object(cache.connection(), &filters, OutputFormat::Jsonl).unwrap();
    let meta: serde_json::Value = serde_json::from_str(json.lines().next().unwrap()).unwrap();
    assert_eq!(meta["totals"]["time_ns"], 100, "{meta}");
}

#[test]
fn lex_entry_allomorph_report_keeps_its_measured_attempts() {
    let mut fact = synthetic_fact(
        pg_stats::ObjectKind::LexEntry,
        Some(("entry-a:0", "Entry Allo")),
        4,
    );
    fact.object_key = "entry-a".to_string();
    fact.object_label = "Entry A".to_string();
    let cache = synthetic_stats_cache("hc", vec![fact]);
    let filters = Filters {
        kind: Some("lex_entry".to_string()),
        ..Filters::default()
    };
    let json = render_allomorph(cache.connection(), &filters, OutputFormat::Jsonl).unwrap();
    let row: serde_json::Value = serde_json::from_str(json.lines().nth(1).unwrap()).unwrap();
    assert_eq!(row["attempts"], 4, "{row}");
    let text = render_allomorph(cache.connection(), &filters, OutputFormat::Text).unwrap();
    let header = text.lines().find(|line| line.contains("label")).unwrap();
    assert!(header.contains("attempts"), "{text}");
}

#[test]
fn never_fires_keeps_attempt_denominators_within_rule_kind() {
    let mut morph = synthetic_fact(pg_stats::ObjectKind::MorphRule, None, 2_000);
    morph.outputs = 0;
    let mut phon = synthetic_fact(pg_stats::ObjectKind::PhonRule, None, 3_000);
    phon.object_key = "phon-a".to_string();
    phon.object_label = "Phon A".to_string();
    phon.outputs = 0;
    let cache = synthetic_stats_cache("hc", vec![morph, phon]);
    let json =
        render_never_fires(cache.connection(), &Filters::default(), OutputFormat::Jsonl).unwrap();
    let meta: serde_json::Value = serde_json::from_str(json.lines().next().unwrap()).unwrap();
    assert!(meta["totals"]["attempts"].is_null(), "{meta}");
    assert_eq!(meta["totals"]["attempts_by_kind"]["morph_rule"], 2_000);
    assert_eq!(meta["totals"]["attempts_by_kind"]["phon_rule"], 3_000);
}

#[test]
fn foma_never_fires_reports_that_the_measurement_is_unsupported() {
    let cache = synthetic_stats_cache("foma", vec![]);
    let text =
        render_never_fires(cache.connection(), &Filters::default(), OutputFormat::Text).unwrap();
    assert!(
        text.contains("engine=foma cannot measure never-fires"),
        "{text}"
    );
    let json =
        render_never_fires(cache.connection(), &Filters::default(), OutputFormat::Jsonl).unwrap();
    let meta: serde_json::Value = serde_json::from_str(json.lines().next().unwrap()).unwrap();
    assert_eq!(
        meta["unmeasured"]["never_fires"],
        "engine=foma cannot measure it"
    );
    let default = render_default(cache.connection(), &Filters::default()).unwrap();
    assert!(
        default.contains("engine=foma cannot measure never-fires"),
        "{default}"
    );
}

#[test]
fn object_text_groups_kinds_without_an_opt_in_flag() {
    let mut phon = synthetic_fact(pg_stats::ObjectKind::PhonRule, None, 3);
    phon.object_key = "phon-a".to_string();
    phon.object_label = "Phon A".to_string();
    let cache = synthetic_stats_cache(
        "hc",
        vec![
            synthetic_fact(pg_stats::ObjectKind::MorphRule, None, 4),
            phon,
        ],
    );
    let text = render_object(cache.connection(), &Filters::default(), OutputFormat::Text).unwrap();
    assert!(text.contains("== morph_rule =="), "{text}");
    assert!(text.contains("== phon_rule =="), "{text}");
}

#[test]
fn default_view_applies_object_filters_instead_of_discarding_them() {
    let mut phon = synthetic_fact(pg_stats::ObjectKind::PhonRule, None, 3);
    phon.object_key = "phon-a".to_string();
    phon.object_label = "Phon A".to_string();
    let cache = synthetic_stats_cache(
        "hc",
        vec![
            synthetic_fact(pg_stats::ObjectKind::MorphRule, None, 4),
            phon,
        ],
    );
    let text = render_default(
        cache.connection(),
        &Filters {
            kind: Some("morph_rule".to_string()),
            sort: Some(pg_stats::SortKey::Attempts),
            ..Filters::default()
        },
    )
    .unwrap();
    assert!(text.contains("== morph_rule =="), "{text}");
    assert!(!text.contains("== phon_rule =="), "{text}");
}

#[test]
fn counter_sort_requires_one_kind() {
    for group_args in [vec!["--group", "object"], vec![]] {
        let mut args = vec!["unused.xml".to_string()];
        args.extend(group_args.into_iter().map(str::to_string));
        args.extend(["--sort".to_string(), "attempts".to_string()]);
        let err = run_stats(&args).expect_err("cross-kind attempt sorting must be rejected");
        assert!(err.contains("--kind"), "{err}");
    }
}

#[test]
fn stats_read_rejects_a_cache_with_multiple_engines() {
    let dir = scratch_dir("mixed-engine-read");
    let path = dir.join("cache.sqlite3");
    let mut outcome = pg_stats::StatsCache::open(&path, "mixed-hash").unwrap();
    let hc = pg_stats::RunMetadata {
        build_info: "test".to_string(),
        fwdata_path: "x".to_string(),
        grammar_hash: "mixed-hash".to_string(),
        engine: "hc".to_string(),
        options_hash: "opts".to_string(),
        options_json: "{}".to_string(),
        created_utc: "unix:0".to_string(),
        step_cap: None,
    };
    outcome
        .cache
        .flush(
            &hc,
            &[pg_stats::WordRecord {
                form: "hc-word".to_string(),
                elapsed_ns: 1,
                attempts: 1,
                passes: 1,
                capped: false,
                timed_out: false,
                invalid_shape: false,
                facts: vec![],
            }],
        )
        .unwrap();
    // Bypass the new write-side guard to model a pre-existing legacy mixed cache.
    outcome
        .cache
        .connection()
        .execute(
            "INSERT INTO run (schema_version, counter_semantics, build_info, fwdata_path, grammar_hash, engine, options_hash, options_json, created_utc, word_count, total_elapsed_ns)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                pg_stats::SCHEMA_VERSION,
                pg_stats::COUNTER_SEMANTICS_VERSION,
                "test",
                "x",
                "mixed-hash",
                "foma",
                "opts",
                "{}",
                "unix:1",
                0i64,
                0i64,
            ],
        )
        .unwrap();
    drop(outcome);

    let args = vec![
        "unused.xml".to_string(),
        "--group".to_string(),
        "word".to_string(),
        "--word".to_string(),
        "missing".to_string(),
        "--cache".to_string(),
        path.to_string_lossy().into_owned(),
    ];
    let err = run_stats(&args).expect_err("mixed-engine cache reads must be hard errors");
    assert!(err.contains("engine"), "{err}");
}

#[test]
fn stats_read_rejects_a_legacy_cache_with_multiple_grammar_hashes() {
    let dir = scratch_dir("mixed-grammar-read");
    let path = dir.join("cache.sqlite3");
    let mut outcome = pg_stats::StatsCache::open(&path, "hash-a").unwrap();
    let run = pg_stats::RunMetadata {
        build_info: "test".to_string(),
        fwdata_path: "x".to_string(),
        grammar_hash: "hash-a".to_string(),
        engine: "hc".to_string(),
        options_hash: "opts".to_string(),
        options_json: "{}".to_string(),
        created_utc: "unix:0".to_string(),
        step_cap: None,
    };
    outcome
        .cache
        .flush(
            &run,
            &[pg_stats::WordRecord {
                form: "a".to_string(),
                elapsed_ns: 1,
                attempts: 1,
                passes: 1,
                capped: false,
                timed_out: false,
                invalid_shape: false,
                facts: vec![],
            }],
        )
        .unwrap();
    outcome
        .cache
        .connection()
        .execute(
            "INSERT INTO run (schema_version, counter_semantics, build_info, fwdata_path, grammar_hash, engine, options_hash, options_json, created_utc, word_count, total_elapsed_ns)
                 VALUES (?1, ?2, 'test', 'x', 'hash-b', 'hc', 'opts', '{}', 'unix:1', 0, 0)",
            rusqlite::params![pg_stats::SCHEMA_VERSION, pg_stats::COUNTER_SEMANTICS_VERSION],
        )
        .unwrap();
    drop(outcome);
    let args = vec![
        "unused.xml".to_string(),
        "--group".to_string(),
        "word".to_string(),
        "--cache".to_string(),
        path.to_string_lossy().into_owned(),
    ];
    let err = run_stats(&args).expect_err("mixed-grammar cache reads must be hard errors");
    assert!(err.contains("grammar"), "{err}");
}

#[test]
fn work_budget_cache_refuses_missing_semantics_and_changed_work_limits() {
    for options in ["{}", r#"{"search_budget_semantics":1,"work_cap":100}"#] {
        let dir = scratch_dir("old-work-budget");
        let path = dir.join("cache.sqlite3");
        let cache = seed_policy_cache(&path, options);
        let err = refuse_if_work_cap_differs(&cache, &path, 100).unwrap_err();
        assert!(err.contains("search-budget semantics"), "{err}");
    }
    let dir = scratch_dir("work-budget-policy");
    let path = dir.join("cache.sqlite3");
    let cache = seed_policy_cache(&path, r#"{"search_budget_semantics":2,"work_cap":100}"#);
    refuse_if_work_cap_differs(&cache, &path, 100).unwrap();
    let err = refuse_if_work_cap_differs(&cache, &path, 200).unwrap_err();
    assert!(err.contains("work cap"), "{err}");
}

#[test]
fn work_budget_cache_reuses_equal_limits_and_refuses_before_truncating_tsv() {
    let (grammar_xml, word) = primary_fixture();
    let dir = scratch_dir("work-budget-tsv");
    let cache_path = dir.join("cache.sqlite3");
    let cache = cache_path.to_str().unwrap();
    let (args, out_path) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &[
            "--stats",
            "--step-cap",
            "unbounded",
            "--work-cap",
            "10000000",
            "--cache",
            cache,
        ],
    );
    crate::run_batch(&args).unwrap();
    assert!(!fs::read_to_string(&out_path).unwrap().is_empty());
    crate::run_batch(&args).unwrap();
    let before = fs::read_to_string(&out_path).unwrap();
    let conn = rusqlite::Connection::open(&cache_path).unwrap();
    let recomputed: i64 = conn
        .query_row(
            "SELECT word_count FROM run ORDER BY run_id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        recomputed, 0,
        "same effective caps must reuse the completed word"
    );
    let mut changed = args.clone();
    let value = changed.iter().position(|a| a == "--work-cap").unwrap() + 1;
    changed[value] = "10000001".into();
    let err = crate::run_batch(&changed).unwrap_err();
    assert!(err.contains("work cap"), "{err}");
    assert_eq!(fs::read_to_string(&out_path).unwrap(), before);
    conn.execute("UPDATE run SET options_json = '{}'", [])
        .unwrap();
    let err = crate::run_batch(&args).unwrap_err();
    assert!(err.contains("search-budget semantics"), "{err}");
    assert_eq!(fs::read_to_string(&out_path).unwrap(), before);
}

#[test]
fn frozen_batch_manifest_records_the_exact_run_input_and_cache_identity() {
    let dir = scratch_dir("frozen-manifest");
    let (grammar_xml, word) = primary_fixture();
    let cache_path = dir.join("frozen-cache.sqlite3");
    let manifest_path = dir.join("stats-run.json");
    let words = vec![word.clone(), format!("{word}x")];
    let words_text = format!("{}\n", words.join("\n"));
    let cache = cache_path.to_string_lossy().into_owned();
    let manifest = manifest_path.to_string_lossy().into_owned();
    let (args, out_path) = run_batch_args(
        &dir,
        &grammar_xml,
        &words_text,
        &[
            "--stats",
            "--cache",
            &cache,
            "--stats-manifest",
            &manifest,
            "--threads",
            "1",
            "--step-cap",
            "unbounded",
        ],
    );

    crate::run_batch(&args).expect("frozen batch stats run");
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).expect("manifest JSON");
    assert_eq!(value["format"], "pangloss-batch-stats-manifest");
    assert_eq!(value["version"], 1);
    assert_eq!(value["source"]["kind"], "hc-xml");
    assert!(value["source"]["compile_options_json"].is_null());
    assert_eq!(
        value["source"]["source_sha256"],
        pg_assess::source_sha256(grammar_xml.as_bytes())
    );
    assert_eq!(
        value["source"]["grammar_hash"],
        sha256_hex(grammar_xml.as_bytes())
    );
    assert_eq!(
        value["source"]["model_fingerprint"].as_str().unwrap().len(),
        "sha256:".len() + 64
    );
    assert_eq!(value["compiler"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["cache"]["schema_version"], pg_stats::SCHEMA_VERSION);
    assert_eq!(
        value["cache"]["counter_semantics_version"],
        pg_stats::COUNTER_SEMANTICS_VERSION
    );
    assert_eq!(value["run"]["engine"], "hc");
    assert!(value["run"]["id"].as_i64().unwrap() > 0);
    assert_eq!(value["batch"]["threads"], 1);
    assert_eq!(value["batch"]["step_cap"], "unbounded");
    assert_eq!(value["input"]["words"], serde_json::json!(words));
    assert_eq!(value["input"]["word_count"], 2);
    assert_eq!(
        value["input"]["word_list_sha256"],
        crate::stats_manifest::sha256_digest(&serde_json::to_vec(&words).unwrap())
    );
    assert_eq!(value["completion"]["requested"], 2);
    assert_eq!(value["completion"]["complete"], 1);
    assert_eq!(value["completion"]["incomplete"], 0);
    assert_eq!(value["completion"]["invalid_shape"], 1);
    assert_eq!(value["completion"]["missing"], 0);
    assert_eq!(value["completion"]["words"][0]["form"], words[0]);
    assert_eq!(value["completion"]["words"][0]["status"], "complete");
    assert_eq!(value["completion"]["words"][1]["form"], words[1]);
    assert_eq!(value["completion"]["words"][1]["status"], "invalid_shape");
    assert!(out_path.exists());

    let conn = rusqlite::Connection::open(&cache_path).unwrap();
    let run_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM run", [], |row| row.get(0))
        .unwrap();
    let run_word_count: i64 = conn
        .query_row(
            "SELECT word_count FROM run WHERE run_id = ?1",
            [value["run"]["id"].as_i64().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    let owning_words: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM word WHERE run_id = ?1",
            [value["run"]["id"].as_i64().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(run_count, 1, "one manifest names one cache run");
    assert_eq!(run_word_count, 2);
    assert_eq!(owning_words, 2);
    drop(conn);

    let bytes = fs::read(&cache_path).unwrap();
    assert_eq!(value["cache"]["bytes"], bytes.len() as u64);
    assert_eq!(
        value["cache"]["sha256"],
        crate::stats_manifest::sha256_digest(&bytes)
    );
}

#[test]
fn frozen_batch_manifest_refuses_reuse_duplicates_and_start_resume() {
    let dir = scratch_dir("frozen-manifest-refusal");
    let (grammar_xml, word) = primary_fixture();
    let cache_path = dir.join("cache.sqlite3");
    let manifest_path = dir.join("stats-run.json");
    let cache = cache_path.to_string_lossy().into_owned();
    let manifest = manifest_path.to_string_lossy().into_owned();
    let (args, out_path) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--cache", &cache, "--stats-manifest", &manifest],
    );
    crate::run_batch(&args).expect("first frozen run");
    let before_cache = fs::read(&cache_path).unwrap();
    let before_manifest = fs::read(&manifest_path).unwrap();
    let before_tsv = fs::read(&out_path).unwrap();

    let second_manifest = dir.join("second-stats-run.json");
    let mut second_args = args.clone();
    let manifest_index = second_args
        .iter()
        .position(|arg| arg == "--stats-manifest")
        .unwrap()
        + 1;
    second_args[manifest_index] = second_manifest.to_string_lossy().into_owned();
    let err = crate::run_batch(&second_args).expect_err("a frozen cache cannot be reused");
    assert!(err.contains("initially empty"), "{err}");
    assert_eq!(fs::read(&cache_path).unwrap(), before_cache);
    assert_eq!(fs::read(&manifest_path).unwrap(), before_manifest);
    assert_eq!(fs::read(&out_path).unwrap(), before_tsv);

    let duplicate_dir = scratch_dir("frozen-manifest-duplicate-word");
    let duplicate_cache = duplicate_dir.join("cache.sqlite3");
    let duplicate_manifest = duplicate_dir.join("stats-run.json");
    let duplicate_cache_text = duplicate_cache.to_string_lossy().into_owned();
    let duplicate_manifest_text = duplicate_manifest.to_string_lossy().into_owned();
    let (duplicate_args, duplicate_out) = run_batch_args(
        &duplicate_dir,
        &grammar_xml,
        &format!("{word}\n{word}\n"),
        &[
            "--stats",
            "--cache",
            &duplicate_cache_text,
            "--stats-manifest",
            &duplicate_manifest_text,
        ],
    );
    let err = crate::run_batch(&duplicate_args).expect_err("duplicate forms overwrite cache rows");
    assert!(err.contains("duplicate word"), "{err}");
    assert!(!duplicate_out.exists());
    assert!(!duplicate_cache.exists());
    assert!(!duplicate_manifest.exists());

    let resume_dir = scratch_dir("frozen-manifest-resume");
    let resume_cache = resume_dir.join("cache.sqlite3");
    let resume_manifest = resume_dir.join("stats-run.json");
    let resume_cache_text = resume_cache.to_string_lossy().into_owned();
    let resume_manifest_text = resume_manifest.to_string_lossy().into_owned();
    let (resume_args, resume_out) = run_batch_args(
        &resume_dir,
        &grammar_xml,
        &format!("{word}\n"),
        &[
            "--stats",
            "--cache",
            &resume_cache_text,
            "--stats-manifest",
            &resume_manifest_text,
            "--start",
            "1",
        ],
    );
    let err = crate::run_batch(&resume_args).expect_err("frozen input cannot resume a partial run");
    assert!(err.contains("--start"), "{err}");
    assert!(!resume_out.exists());
    assert!(!resume_cache.exists());
    assert!(!resume_manifest.exists());
}

#[test]
fn frozen_batch_manifest_requires_stats_and_an_explicit_cache() {
    let dir = scratch_dir("frozen-manifest-requires-flags");
    let (grammar_xml, _) = primary_fixture();
    let manifest_path = dir.join("stats-run.json");
    let manifest = manifest_path.to_string_lossy().into_owned();

    let (no_stats, out_path) = run_batch_args(
        &dir,
        &grammar_xml,
        "word\n",
        &["--stats-manifest", &manifest],
    );
    let err = crate::run_batch(&no_stats).expect_err("manifest requires --stats");
    assert!(err.contains("--stats-manifest requires --stats"), "{err}");
    assert!(!out_path.exists());

    let stats_dir = scratch_dir("frozen-manifest-requires-cache");
    let (no_cache, stats_out) = run_batch_args(
        &stats_dir,
        &grammar_xml,
        "word\n",
        &["--stats", "--stats-manifest", &manifest],
    );
    let err = crate::run_batch(&no_cache).expect_err("manifest requires explicit --cache");
    assert!(err.contains("explicit --cache"), "{err}");
    assert!(!stats_out.exists());
}

#[test]
fn frozen_batch_manifest_tracks_grammar_compiler_and_option_identities() {
    fn run_frozen(
        dir: &std::path::Path,
        grammar_xml: &str,
        word: &str,
        extra: &[&str],
    ) -> serde_json::Value {
        let cache_path = dir.join("cache.sqlite3");
        let manifest_path = dir.join("stats-run.json");
        let cache = cache_path.to_string_lossy().into_owned();
        let manifest = manifest_path.to_string_lossy().into_owned();
        let mut args = vec!["--stats", "--cache", &cache, "--stats-manifest", &manifest];
        args.extend_from_slice(extra);
        let extra_args: Vec<&str> = args;
        let (batch_args, _) = run_batch_args(dir, grammar_xml, &format!("{word}\n"), &extra_args);
        crate::run_batch(&batch_args).expect("frozen stats run");
        serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap()
    }

    let (grammar_a, word_a) = primary_fixture();
    let (grammar_b, word_b) = secondary_fixture();
    let first = run_frozen(
        &scratch_dir("frozen-identity-a"),
        &grammar_a,
        &word_a,
        &["--threads", "1"],
    );
    let second = run_frozen(
        &scratch_dir("frozen-identity-b"),
        &grammar_b,
        &word_b,
        &["--threads", "2", "--word-timeout-ms", "10000"],
    );
    assert_ne!(
        first["source"]["source_sha256"],
        second["source"]["source_sha256"]
    );
    assert_ne!(
        first["source"]["grammar_hash"],
        second["source"]["grammar_hash"]
    );
    assert_ne!(
        first["source"]["model_fingerprint"],
        second["source"]["model_fingerprint"]
    );
    assert_eq!(first["compiler"]["version"], second["compiler"]["version"]);
    assert_eq!(
        first["compiler"]["build_identity"],
        second["compiler"]["build_identity"]
    );
    assert_eq!(first["cache"]["schema_version"], pg_stats::SCHEMA_VERSION);
    assert_eq!(
        first["cache"]["counter_semantics_version"],
        pg_stats::COUNTER_SEMANTICS_VERSION
    );
    assert_eq!(first["batch"]["threads"], 1);
    assert_eq!(second["batch"]["threads"], 2);
    assert_eq!(second["batch"]["word_timeout_ms"], 10000);
    assert_ne!(first["run"]["options_hash"], second["run"]["options_hash"]);
}

#[test]
fn frozen_batch_manifest_does_not_replace_an_existing_output() {
    let dir = scratch_dir("frozen-manifest-existing-output");
    let (grammar_xml, word) = primary_fixture();
    let cache_path = dir.join("cache.sqlite3");
    let manifest_path = dir.join("stats-run.json");
    let cache = cache_path.to_string_lossy().into_owned();
    let manifest = manifest_path.to_string_lossy().into_owned();
    let (args, out_path) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &["--stats", "--cache", &cache, "--stats-manifest", &manifest],
    );
    fs::write(&manifest_path, b"keep-existing-manifest").unwrap();

    let err = crate::run_batch(&args).expect_err("manifest publication must not overwrite");
    assert!(err.contains("output_exists"), "{err}");
    assert_eq!(fs::read(&manifest_path).unwrap(), b"keep-existing-manifest");
    assert!(!cache_path.exists());
    assert!(!out_path.exists());
}

#[test]
fn frozen_batch_manifest_records_partial_searches_and_refuses_missing_rows() {
    let dir = scratch_dir("frozen-manifest-partial");
    let (grammar_xml, word) = primary_fixture();
    let cache_path = dir.join("cache.sqlite3");
    let manifest_path = dir.join("stats-run.json");
    let cache = cache_path.to_string_lossy().into_owned();
    let manifest = manifest_path.to_string_lossy().into_owned();
    let (args, _) = run_batch_args(
        &dir,
        &grammar_xml,
        &format!("{word}\n"),
        &[
            "--stats",
            "--cache",
            &cache,
            "--stats-manifest",
            &manifest,
            "--word-timeout-ms",
            "0",
        ],
    );
    crate::run_batch(&args).expect("timed-out parser outcome is still a frozen observation");
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    assert_eq!(value["completion"]["requested"], 1);
    assert_eq!(value["completion"]["complete"], 0);
    assert_eq!(value["completion"]["incomplete"], 1);
    assert_eq!(value["completion"]["words"][0]["status"], "incomplete");
    assert_eq!(value["completion"]["words"][0]["timed_out"], true);

    let cancelled_dir = scratch_dir("frozen-manifest-cancel-before-flush");
    let cancelled_cache = cancelled_dir.join("cache.sqlite3");
    let cancelled_manifest = cancelled_dir.join("stats-run.json");
    let (cancelled_args, _) =
        run_batch_args(&cancelled_dir, &grammar_xml, &format!("{word}\n"), &[]);
    let prepared = prepare_frozen_batch_stats_hc(
        &cancelled_args[0],
        std::slice::from_ref(&word),
        crate::DEFAULT_STEP_CAP,
        crate::resolved_work_cap(crate::DEFAULT_STEP_CAP, None),
        None,
        false,
        false,
        cancelled_cache.to_str().unwrap(),
        &crate::stats_manifest::SourceIdentity::from_hc_xml(&grammar_xml)
            .unwrap()
            .grammar_hash,
    )
    .expect("prepare the fresh frozen cache");
    drop(prepared);
    assert!(!cancelled_manifest.exists());
    let cancelled_conn = rusqlite::Connection::open(&cancelled_cache).unwrap();
    let run_count: i64 = cancelled_conn
        .query_row("SELECT COUNT(*) FROM run", [], |row| row.get(0))
        .unwrap();
    assert_eq!(run_count, 0, "an abandoned parse publishes no run");

    let missing_dir = scratch_dir("frozen-manifest-missing-word");
    let missing_cache = missing_dir.join("cache.sqlite3");
    let (missing_args, _) = run_batch_args(&missing_dir, &grammar_xml, &format!("{word}\n"), &[]);
    let prepared = prepare_frozen_batch_stats_hc(
        &missing_args[0],
        std::slice::from_ref(&word),
        crate::DEFAULT_STEP_CAP,
        crate::resolved_work_cap(crate::DEFAULT_STEP_CAP, None),
        None,
        false,
        false,
        missing_cache.to_str().unwrap(),
        &crate::stats_manifest::SourceIdentity::from_hc_xml(&grammar_xml)
            .unwrap()
            .grammar_hash,
    )
    .expect("prepare the fresh frozen cache");
    let source = crate::stats_manifest::SourceIdentity::from_hc_xml(&grammar_xml).unwrap();
    let err = finish_frozen_batch_stats_hc(
        prepared,
        Vec::new(),
        &missing_dir.join("stats-run.json"),
        source,
        crate::stats_manifest::BatchOptions {
            engine: "hc",
            threads: 1,
            step_cap: crate::DEFAULT_STEP_CAP,
            work_cap: crate::resolved_work_cap(crate::DEFAULT_STEP_CAP, None),
            search_budget_semantics: SEARCH_BUDGET_SEMANTICS,
            word_timeout_ms: None,
            guess: false,
            always_enforce_final_templates: false,
            start: 0,
            analyses_requested: false,
        },
    )
    .expect_err("a missing parse result cannot be frozen");
    assert!(err.contains("missing stats record"), "{err}");
    assert!(!missing_dir.join("stats-run.json").exists());
}

#[test]
fn frozen_batch_manifest_rejects_colliding_paths_before_creating_outputs() {
    let dir = scratch_dir("frozen-manifest-collisions");
    let (grammar_xml, word) = primary_fixture();
    let grammar_path = dir.join("grammar.xml");
    let words_path = dir.join("words.txt");
    let out_path = dir.join("out.tsv");
    fs::write(&grammar_path, &grammar_xml).unwrap();
    fs::write(&words_path, format!("{word}\n")).unwrap();
    for (tag, collision) in [
        ("grammar", grammar_path.clone()),
        ("words", words_path.clone()),
        ("tsv", out_path.clone()),
    ] {
        let cache_path = dir.join(format!("{tag}-cache.sqlite3"));
        let cache = cache_path.to_string_lossy().into_owned();
        let collision = collision.to_string_lossy().into_owned();
        let args = vec![
            grammar_path.to_string_lossy().into_owned(),
            words_path.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
            "--stats".into(),
            "--cache".into(),
            cache.clone(),
            "--stats-manifest".into(),
            collision,
        ];
        let before_grammar = fs::read(&grammar_path).unwrap();
        let before_words = fs::read(&words_path).unwrap();
        let err = crate::run_batch(&args).expect_err("manifest cannot alias batch paths");
        assert!(err.contains("path collision"), "{err}");
        assert_eq!(fs::read(&grammar_path).unwrap(), before_grammar);
        assert_eq!(fs::read(&words_path).unwrap(), before_words);
        assert!(!cache_path.exists());
    }
}
