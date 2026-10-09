//! Compares stored-analysis keys plus captured status; absent native root position/category prevents a full C# structured-identity parity claim.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct StoredMorph {
    allomorph: String,
    msa: String,
    inflection_type: Option<String>,
}

type StoredKey = Vec<StoredMorph>;

#[derive(Deserialize)]
struct EngineObservation {
    stored_analysis_keys: Vec<StoredKey>,
    engine_error: Option<String>,
}

#[derive(Deserialize)]
struct Word {
    word: String,
    xample: EngineObservation,
    csharp_hc: EngineObservation,
}

#[derive(Deserialize)]
struct Case {
    measurement_protocol: String,
    words: Vec<Word>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct Difference {
    word: String,
    key: Option<StoredKey>,
    csharp_count: usize,
    pangloss_count: usize,
    csharp_error: Option<String>,
    pangloss_invalid_shape: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedDifference {
    difference: Difference,
    ledger: String,
    reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DifferenceTable {
    schema_version: u32,
    comparison_profile: String,
    expected_differences: Vec<ExpectedDifference>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct MinimumLoss {
    case: String,
    word: String,
    key: StoredKey,
    xample_count: usize,
    pangloss_count: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MinimumException {
    loss: MinimumLoss,
    reason: String,
    ledger: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MinimumExceptionTable {
    schema_version: u32,
    exceptions: Vec<MinimumException>,
}

fn check_minimum_losses(actual: &[MinimumLoss], expected: &[MinimumLoss]) -> Result<(), String> {
    let actual = actual.iter().collect::<std::collections::BTreeSet<_>>();
    let expected_set = expected.iter().collect::<std::collections::BTreeSet<_>>();
    if expected_set.len() != expected.len() {
        return Err("duplicate expected minimum exceptions".into());
    }
    if actual != expected_set {
        return Err(format!(
            "unlisted XAMPLE losses: {:?}; disappeared exceptions: {:?}",
            actual.difference(&expected_set).collect::<Vec<_>>(),
            expected_set.difference(&actual).collect::<Vec<_>>()
        ));
    }
    Ok(())
}

fn differences(
    word: &str,
    hc: &BTreeMap<StoredKey, usize>,
    pg: &BTreeMap<StoredKey, usize>,
    error: Option<&str>,
    invalid_shape: bool,
) -> Vec<Difference> {
    let keys = hc
        .keys()
        .chain(pg.keys())
        .collect::<std::collections::BTreeSet<_>>();
    let mut result = Vec::new();
    for key in keys {
        let csharp_count = hc.get(key).copied().unwrap_or_default();
        let pangloss_count = pg.get(key).copied().unwrap_or_default();
        if csharp_count != pangloss_count {
            result.push(Difference {
                word: word.into(),
                key: Some(key.clone()),
                csharp_count,
                pangloss_count,
                csharp_error: None,
                pangloss_invalid_shape: false,
            });
        }
    }
    if error.is_some() || invalid_shape {
        result.push(Difference {
            word: word.into(),
            key: None,
            csharp_count: 0,
            pangloss_count: 0,
            csharp_error: error.map(str::to_string),
            pangloss_invalid_shape: invalid_shape,
        });
    }
    result
}

fn check_differences(actual: &[Difference], expected: &[Difference]) -> Result<(), String> {
    let actual = actual.iter().collect::<std::collections::BTreeSet<_>>();
    let expected_set = expected.iter().collect::<std::collections::BTreeSet<_>>();
    if expected_set.len() != expected.len() {
        return Err("duplicate expected difference rows".into());
    }
    if actual != expected_set {
        return Err(format!(
            "unlisted: {:?}; disappeared: {:?}",
            actual.difference(&expected_set).collect::<Vec<_>>(),
            expected_set.difference(&actual).collect::<Vec<_>>()
        ));
    }
    Ok(())
}

fn multiset(keys: impl IntoIterator<Item = StoredKey>) -> BTreeMap<StoredKey, usize> {
    let mut result = BTreeMap::new();
    for key in keys {
        assert!(!key.is_empty(), "an empty analysis is not a stored key");
        *result.entry(key).or_default() += 1;
    }
    result
}

fn projected_keys(
    outcome: &pg_parse::ParseOutcome,
    grammar: &pg_grammar::model::Grammar,
) -> BTreeMap<StoredKey, usize> {
    multiset(outcome.structured.iter().map(|analysis| {
        pg_parse::project_parse_analysis(analysis, grammar)
            .expect("confirmed analysis must retain complete source identities")
            .morphs
            .into_iter()
            .map(|morph| {
                assert!(
                    morph.guessed_string.is_none(),
                    "measured words are not guessed"
                );
                StoredMorph {
                    allomorph: morph.form.expect("authored allomorph GUID"),
                    msa: morph.msa.expect("authored MSA GUID"),
                    inflection_type: morph.infl_type,
                }
            })
            .collect()
    }))
}

#[test]
fn every_measured_underdefined_case_preserves_xample_stored_keys() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../conformance-staging/underdefined");
    let mut cases = std::fs::read_dir(&root)
        .expect("measured underdefined staging must exist")
        .map(|entry| entry.expect("read case").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    cases.sort();
    assert!(!cases.is_empty(), "a missing inventory cannot pass");
    let mut failures = Vec::new();
    let mut checked = 0;
    for path in &cases {
        let name = path.file_name().unwrap().to_str().unwrap();
        let text = std::fs::read_to_string(path.join("words.yaml")).expect("read words");
        let provenance = text.lines().next().expect("oracle provenance");
        assert!(provenance.starts_with("# oracle-provenance:"));
        assert!(provenance.contains("XAMPLE") && provenance.contains("Machine/HC"));
        let json = text
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        let case: Case = serde_json::from_str(&json).expect("measured words schema");
        assert_eq!(case.measurement_protocol, "underdefined-stored-keys-v1");
        let (snapshot, report) = pg_fwdata::import_file(&path.join("fieldworks/project.fwdata"))
            .expect("import saved project");
        let compiled = pg_grammar::compile::compile_project_with_options_and_import_warnings(
            &snapshot,
            pg_grammar::compile::CompileOptions::default(),
            report.warnings,
        )
        .expect("saved underdefined project must compile without a semantic-loss override");
        let morpher = pg_parse::Morpher::new(&compiled.grammar, 1_000_000);
        let mut observed_differences = Vec::new();
        let mut observed_losses = Vec::new();
        for row in case.words {
            checked += 1;
            assert!(
                row.xample.engine_error.is_none(),
                "XAMPLE errors are not minimum answers"
            );
            let outcome = morpher.parse_word(&row.word);
            assert!(
                !outcome.capped && !outcome.timed_out,
                "{name}/{} incomplete",
                row.word
            );
            let actual = projected_keys(&outcome, &compiled.grammar);
            let minimum = multiset(row.xample.stored_analysis_keys);
            let hc = multiset(row.csharp_hc.stored_analysis_keys);
            observed_differences.extend(differences(
                &row.word,
                &hc,
                &actual,
                row.csharp_hc.engine_error.as_deref(),
                outcome.invalid_shape,
            ));
            eprintln!(
                "UNDERDEFINED_ROW {}",
                serde_json::json!({"case":name,"word":row.word,"pangloss":actual.iter().collect::<Vec<_>>(),"xample":minimum.iter().collect::<Vec<_>>(),"csharp_hc":hc.iter().collect::<Vec<_>>(),"hc_error":row.csharp_hc.engine_error,"invalid_shape":outcome.invalid_shape})
            );
            for (key, expected) in minimum {
                let count = actual.get(&key).copied().unwrap_or_default();
                if count < expected {
                    observed_losses.push(MinimumLoss {
                        case: name.into(),
                        word: row.word.clone(),
                        key,
                        xample_count: expected,
                        pangloss_count: count,
                    });
                }
            }
        }
        let exceptions: MinimumExceptionTable = serde_json::from_str(
            &std::fs::read_to_string(path.join("xample-minimum-exceptions.json"))
                .expect("each measured case must explicitly list minimum exceptions"),
        )
        .expect("minimum exception table schema");
        assert_eq!(exceptions.schema_version, 1);
        let expected_losses = exceptions
            .exceptions
            .into_iter()
            .map(|row| {
                assert_eq!(row.loss.case, name);
                assert!(!row.reason.trim().is_empty());
                assert_eq!(row.ledger, "078-xample-minimum-authored-phonology.md");
                assert!(root
                    .join("../../docs/divergences")
                    .join(row.ledger)
                    .is_file());
                row.loss
            })
            .collect::<Vec<_>>();
        if let Err(error) = check_minimum_losses(&observed_losses, &expected_losses) {
            failures.push(format!("{name}: {error}"));
        }
        let table: DifferenceTable = serde_json::from_str(
            &std::fs::read_to_string(path.join("expected-differences.json"))
                .expect("each measured case must have an explicit difference table"),
        )
        .expect("difference table schema");
        assert_eq!(table.schema_version, 1);
        assert_eq!(
            table.comparison_profile,
            "underdefined-stored-keys-and-status/v1"
        );
        let expected = table
            .expected_differences
            .into_iter()
            .map(|row| {
                assert!(!row.reason.trim().is_empty());
                assert!(!row.ledger.contains('/') && !row.ledger.contains('\\'));
                assert!(
                    root.join("../../docs/divergences")
                        .join(&row.ledger)
                        .is_file(),
                    "difference must name a real ledger entry"
                );
                row.difference
            })
            .collect::<Vec<_>>();
        if let Err(error) = check_differences(&observed_differences, &expected) {
            failures.push(format!("{name}: {error}"));
        }
    }
    eprintln!("Measured {} cases and {checked} words", cases.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn difference_tables_reject_unlisted_disappeared_and_changed_source_keys() {
    let key = vec![StoredMorph {
        allomorph: "form".into(),
        msa: "msa".into(),
        inflection_type: None,
    }];
    let hc = multiset([key.clone()]);
    let mut extra = key.clone();
    extra[0].allomorph = "other-form".into();
    let pg = multiset([key.clone(), extra]);
    let listed = differences("probe", &hc, &pg, None, false);
    assert_eq!(listed.len(), 1);
    assert!(
        check_differences(&listed, &[]).is_err(),
        "unlisted difference must fail"
    );
    assert!(
        check_differences(&listed, &listed).is_ok(),
        "the non-vacuity control must pass"
    );
    assert!(
        check_differences(&[], &listed).is_err(),
        "a disappeared difference must fail"
    );
    for field in ["allomorph", "msa", "inflection_type"] {
        let mut changed = key.clone();
        match field {
            "allomorph" => changed[0].allomorph = "changed".into(),
            "msa" => changed[0].msa = "changed".into(),
            "inflection_type" => changed[0].inflection_type = Some("changed".into()),
            _ => unreachable!(),
        }
        let changed = differences("probe", &hc, &multiset([key.clone(), changed]), None, false);
        assert!(
            check_differences(&changed, &listed).is_err(),
            "{field} must discriminate"
        );
    }
    let duplicate = [listed[0].clone(), listed[0].clone()];
    assert!(check_differences(&listed, &duplicate).is_err());
    let status = differences("probe", &hc, &hc, Some("engine failure"), false);
    assert!(check_differences(&status, &[]).is_err());
    assert!(check_differences(&[], &status).is_err());
}
