use super::*;
use pg_foma::differential::IdentityMultiplicity;

fn execute(
    xml: &str,
    words: &str,
    flags: &[&str],
) -> (Result<(), String>, serde_json::Value, String) {
    let dir = tempfile::tempdir().unwrap();
    let grammar = dir.path().join("grammar.xml");
    let input = dir.path().join("words.txt");
    let output = dir.path().join("result.tsv");
    let summary = dir.path().join("summary.json");
    fs::write(&grammar, xml).unwrap();
    fs::write(&input, words).unwrap();
    let mut args = vec![
        grammar.display().to_string(),
        input.display().to_string(),
        "--out".into(),
        output.display().to_string(),
        "--json".into(),
        summary.display().to_string(),
    ];
    args.extend(flags.iter().map(|s| s.to_string()));
    let result = run_fst_verify(&args);
    let json = serde_json::from_slice(&fs::read(summary).unwrap()).unwrap();
    (result, json, fs::read_to_string(output).unwrap())
}

#[test]
fn embedded_tsv_delimiters_are_rejected_before_outputs_are_opened() {
    let dir = tempfile::tempdir().unwrap();
    let grammar = dir.path().join("grammar.xml");
    let words = dir.path().join("words.txt");
    let output = dir.path().join("result.tsv");
    let summary = dir.path().join("summary.json");
    fs::write(&grammar, crate::tests::MINI_GRAMMAR_XML).unwrap();
    for input in ["kat\t kat\n", "kat\rkat\n"] {
        fs::write(&words, input).unwrap();
        for existing in [false, true] {
            if existing {
                fs::write(&output, "preserved TSV").unwrap();
                fs::write(&summary, "preserved JSON").unwrap();
            }
            let args = [
                grammar.display().to_string(),
                words.display().to_string(),
                "--out".into(),
                output.display().to_string(),
                "--json".into(),
                summary.display().to_string(),
            ];
            let error = run_fst_verify(&args).unwrap_err();
            assert!(error.contains("embedded tabs or newlines"), "{error}");
            if existing {
                assert_eq!(fs::read_to_string(&output).unwrap(), "preserved TSV");
                assert_eq!(fs::read_to_string(&summary).unwrap(), "preserved JSON");
                fs::remove_file(&output).unwrap();
                fs::remove_file(&summary).unwrap();
            } else {
                assert!(!output.exists());
                assert!(!summary.exists());
            }
        }
    }
}

#[test]
fn equal_conformance_fixture_preserves_duplicate_occurrences_and_negative_controls() {
    let xml = pg_conformance_fixtures::require_fixture("edge-cases", "rewrite-default-environment")
        .load_grammar_xml();
    let (result, report, tsv) = execute(&xml, "bat\r\nbdt\nbat\n\n", &[]);
    result.unwrap();
    assert_eq!(report["totals"]["equal"], 3);
    assert_eq!(report["totals"]["words"], 3);
    assert_eq!(
        report["construction"]["attempts"][0]["capability_causes"],
        json!([])
    );
    assert_eq!(
        report["construction"]["attempts"][0]["proposal_recall_occurrences"],
        json!([])
    );
    assert_eq!(report["rows"][0]["word"], report["rows"][2]["word"]);
    assert_eq!(report["rows"][2]["index"], 2);
    assert_eq!(tsv.lines().count(), 4);
    assert_eq!(report["grammar_sha256"].as_str().unwrap().len(), 64);
    assert!(report["engines"]["build"]
        .as_str()
        .unwrap()
        .starts_with("pangloss/"));
}

#[test]
fn known_epenthesis_gap_refuses_before_word_comparison() {
    let xml = pg_conformance_fixtures::require_fixture("edge-cases", "rewrite-default-epenthesis")
        .load_grammar_xml();
    let (result, report, tsv) = execute(&xml, "bdt\n", &[]);
    assert!(result.is_err());
    assert_eq!(report["totals"]["fst_refused_grammar"], true);
    assert_eq!(report["totals"]["incomparable"], 1);
    assert_eq!(report["totals"]["missing"], 0);
    assert_eq!(report["rows"][0]["verdict"], "incomparable");
    assert_eq!(report["rows"][0]["missing"], json!([]));
    assert_eq!(report["rows"][0]["extra"], json!([]));
    assert_eq!(
        report["rows"][0]["hc_status"],
        "not_run_fst_refused_grammar"
    );
    assert_eq!(
        report["construction"]["attempts"][0]["refusal"]["kind"],
        "capability"
    );
    assert!(report["compile_refusal"]
        .as_str()
        .unwrap()
        .contains("surface-probe.proposal-recall"));
    assert!(tsv.contains("bdt\tincomparable\t[]\t[]"));
}

#[test]
fn refusal_json_preserves_typed_witness_counts_without_expanding_the_diagnostics() {
    let xml = pg_conformance_fixtures::require_fixture("edge-cases", "featureless-literal-target")
        .load_grammar_xml()
        .replace(
            "<Representation>k</Representation>",
            "<Representation>k</Representation><Representation>x</Representation>",
        );
    let (result, report, _) = execute(&xml, "aka\naka\n", &[]);
    assert!(result.is_err());
    let cause = json!({"kind": "proposal_recall", "details": "ambiguous_segment_spelling"});
    let attempt = &report["construction"]["attempts"][0];
    assert_eq!(attempt["capability_causes"], json!([cause]));
    assert_eq!(
        attempt["proposal_recall_occurrences"],
        json!([{
            "strategy": "tuned-surface-probed", "cause": cause, "occurrences": 2
        }])
    );
    assert_eq!(attempt["states"], serde_json::Value::Null);
    assert_eq!(
        attempt["detail"]
            .as_str()
            .unwrap()
            .matches("construct=")
            .count(),
        1
    );
    assert_eq!(report["totals"]["incomparable"], 2);
}

#[test]
fn caps_and_timeouts_are_incomparable() {
    for flags in [
        vec!["--word-timeout-ms=0"],
        vec!["--work-cap", "1"],
        vec!["--apply-path-cap=0"],
        vec!["--apply-candidate-cap=0"],
    ] {
        let (result, report, _) = execute(crate::tests::MINI_GRAMMAR_XML, "kat\n", &flags);
        result.unwrap();
        assert_eq!(report["totals"]["incomparable"], 1, "{flags:?}: {report}");
        assert_eq!(report["rows"][0]["missing"], json!([]));
        assert_eq!(report["rows"][0]["extra"], json!([]));
    }
}

#[test]
fn invalid_shapes_are_incomparable_and_batch_option_spellings_are_shared() {
    let (result, report, _) = execute(
        crate::tests::MINI_GRAMMAR_XML,
        "kat\nxxx\n",
        &[
            "--work-cap=unbounded",
            "--word-mem-cap-mib=0",
            "--capture-cap-mib=unbounded",
            "--always-enforce-final-templates=false",
        ],
    );
    result.unwrap();
    assert_eq!(report["rows"][0]["verdict"], "equal");
    assert_eq!(report["rows"][1]["verdict"], "incomparable");
    assert_eq!(report["rows"][1]["hc_status"], "invalid_shape");
    assert!(report["options"]["word_mem_cap_bytes"].is_null());
    assert!(report["options"]["capture_cap_bytes"].is_null());
    assert_eq!(report["options"]["always_enforce_final_templates"], false);
}

#[test]
fn refused_construction_retains_rows_and_typed_refusal() {
    let (result, report, _) = execute(
        crate::tests::MINI_GRAMMAR_XML,
        "kat\nkat\n",
        &["--fst-strategy=factored"],
    );
    assert!(result.is_err());
    assert_eq!(report["totals"]["fst_refused_grammar"], true);
    assert_eq!(report["totals"]["incomparable"], 2);
    assert_eq!(
        report["construction"]["attempts"][0]["refusal"]["kind"],
        "not_implemented"
    );
    assert_eq!(
        report["rows"][0]["hc_status"],
        "not_run_fst_refused_grammar"
    );
}

fn identity(key: &str) -> AnalysisIdentity {
    AnalysisIdentity {
        morphemes: vec![Some(key.into())],
        root_index: 0,
        category: None,
    }
}

#[test]
fn full_identity_sets_ignore_multiplicity_and_record_both_difference_directions() {
    let mut row = DifferentialRow::not_implemented(0, "a", "complete");
    row.incomplete = false;
    row.oracle = vec![IdentityMultiplicity {
        identity: identity("root"),
        count: 2,
    }];
    row.confirmed = vec![IdentityMultiplicity {
        identity: identity("root"),
        count: 1,
    }];
    assert_eq!(compare(row.clone()).verdict, Verdict::Equal);
    row.confirmed[0].identity.category = Some("noun".into());
    let both = compare(row.clone());
    assert_eq!(both.verdict, Verdict::Missing);
    assert_eq!(both.missing, vec![identity("root")]);
    assert_eq!(both.extra.len(), 1);
    row.oracle.clear();
    assert_eq!(compare(row.clone()).verdict, Verdict::Extra);
    row.incomplete = true;
    let stopped = compare(row);
    assert_eq!(stopped.verdict, Verdict::Incomparable);
    assert!(stopped.missing.is_empty() && stopped.extra.is_empty());
}

#[test]
fn output_collisions_and_bad_options_are_rejected_before_writes() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    fs::write(&input, "unchanged").unwrap();
    let input = input.display().to_string();
    for tail in [
        vec!["--out", input.as_str()],
        vec!["--json", input.as_str()],
        vec!["--unknown=1"],
        vec!["--word-timeout-ms=no"],
        vec!["--json"],
    ] {
        let mut args = vec![input.clone(), input.clone()];
        args.extend(tail.iter().map(|s| s.to_string()));
        assert!(run_fst_verify(&args).is_err());
        assert_eq!(fs::read_to_string(&input).unwrap(), "unchanged");
    }
    assert!(reject_collisions(&[], &["same", "./same"]).is_err());
    #[cfg(unix)]
    {
        let alias = dir.path().join("alias");
        fs::hard_link(&input, &alias).unwrap();
        assert!(reject_collisions(&[&input], &[alias.to_str().unwrap()]).is_err());
    }
}

#[test]
fn fst_confirmation_stop_cannot_be_reported_as_a_missing_analysis() {
    let grammar = pg_grammar::load(crate::tests::MINI_GRAMMAR_XML).unwrap();
    for stop in ["work", "time", "memory"] {
        let analyzer = pg_foma::composite::compile_analyzer(&grammar).unwrap();
        let (proposer, peeler, owners, _, filter) = analyzer.into_parts_with_morpher();
        let morpher = Morpher::new(&grammar, if stop == "work" { 1 } else { usize::MAX })
            .with_word_timeout((stop == "time").then_some(Duration::ZERO))
            .with_word_mem_cap((stop == "memory").then_some(1));
        let mut analyzer = pg_foma::composite::FomaAnalyzer::from_cached_with_morpher(
            &grammar, proposer, peeler, owners, morpher, filter,
        );
        let row = measure_word(
            &grammar,
            &Morpher::new(&grammar, usize::MAX),
            Some(&mut analyzer),
            0,
            "kat",
            &ApplyBudget::unbounded(),
            None,
        )
        .unwrap();
        assert_eq!(row.hc_status, "complete");
        assert_eq!(row.fst_status, "confirmation_incomplete");
        assert_eq!(compare(row).verdict, Verdict::Incomparable);
    }
}

#[test]
fn hc_memory_stop_is_incomparable_even_when_fst_completes() {
    let grammar = pg_grammar::load(crate::tests::MINI_GRAMMAR_XML).unwrap();
    let mut analyzer = pg_foma::composite::compile_analyzer(&grammar).unwrap();
    let morpher = Morpher::new(&grammar, usize::MAX).with_word_mem_cap(Some(1));
    let row = measure_word(
        &grammar,
        &morpher,
        Some(&mut analyzer),
        0,
        "kat",
        &ApplyBudget::unbounded(),
        None,
    )
    .unwrap();
    assert_eq!(row.hc_status, "memory_capped");
    assert_eq!(row.fst_status, "complete");
    assert_eq!(compare(row).verdict, Verdict::Incomparable);
}

#[test]
fn restricted_confirmation_groups_spend_one_shared_word_budget() {
    let grammar = pg_grammar::load(crate::tests::MINI_GRAMMAR_XML).unwrap();
    let morpher = Morpher::new(&grammar, 10_000);
    let budget = morpher.word_budget();
    let first =
        morpher.parse_word_selected_with_budget("kat", &Default::default(), None, None, &budget);
    assert!(!first.capped && !first.structured.is_empty());
    let mut previous = first.work_steps;
    for _ in 0..10_001 {
        let next = morpher.parse_word_selected_with_budget(
            "kat",
            &Default::default(),
            None,
            None,
            &budget,
        );
        assert!(next.work_steps >= previous);
        if next.capped {
            return;
        }
        assert!(next.work_steps > previous);
        previous = next.work_steps;
    }
    panic!("repeated confirmation must exhaust the single word's allowance");
}
