//! Batch `--out`, `target` and `cell` rows; every expected surface is read from the `request` path or already pinned.

use std::process::Command;

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");
const SNAPSHOT: &str = include_str!("../../../../docs/divergences/evidence/071/kat-snapshot.json");
const ENTRY: &str = "00000000-0000-0000-0000-000000000040";
const MSA: &str = "00000000-0000-0000-0000-000000000042";
const MSA2: &str = "00000000-0000-0000-0000-000000000044";
const SENSE: &str = "00000000-0000-0000-0000-000000000043";
const SENSE2: &str = "00000000-0000-0000-0000-000000000045";
const INFL_ENTRY: &str = "00000000-0000-0000-0000-000000000050";
const INFL_MSA: &str = "00000000-0000-0000-0000-000000000052";
const DERIV_ENTRY: &str = "00000000-0000-0000-0000-000000000070";
const DERIV_MSA: &str = "00000000-0000-0000-0000-000000000072";

/// The kat snapshot with a second stem MSA and sense on the `ranna` entry and a derivational suffix.
fn snapshot() -> String {
    let mut doc: Value = serde_json::from_str(SNAPSHOT).unwrap();
    doc["morphology"]["parserParameters"]["mergeMsas"] = json!(false);
    let entries = doc["lexicon"]["entries"].as_array_mut().unwrap();
    let ranna = &mut entries[1];
    let mut msa2 = ranna["msas"][0].clone();
    msa2["guid"] = json!(MSA2);
    ranna["msas"].as_array_mut().unwrap().push(msa2);
    let mut sense2 = ranna["senses"][0].clone();
    sense2["guid"] = json!(SENSE2);
    sense2["msa"] = json!(MSA2);
    ranna["senses"].as_array_mut().unwrap().push(sense2);
    let mut deriv = entries[2].clone();
    deriv["guid"] = json!(DERIV_ENTRY);
    deriv["allomorphs"][0]["guid"] = json!("00000000-0000-0000-0000-000000000071");
    deriv["allomorphs"][0]["forms"] = json!([{"ws": "fx", "form": "k"}]);
    deriv["msas"] = json!([{
        "kind": "derivational",
        "guid": DERIV_MSA,
        "from_part_of_speech": "00000000-0000-0000-0000-00000000000c",
        "to_part_of_speech": "00000000-0000-0000-0000-00000000000c",
    }]);
    deriv["senses"][0]["guid"] = json!("00000000-0000-0000-0000-000000000073");
    deriv["senses"][0]["msa"] = json!(DERIV_MSA);
    entries.push(deriv);
    doc.to_string()
}

struct Run {
    dir: tempfile::TempDir,
    project: std::path::PathBuf,
}

impl Run {
    fn new() -> Run {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("kat.json");
        std::fs::write(&project, snapshot()).unwrap();
        Run { dir, project }
    }

    fn lines(&self, rows: &[Value]) -> std::path::PathBuf {
        let path = self.dir.path().join("in.jsonl");
        let text: String = rows.iter().map(|r| r.to_string() + "\n").collect();
        std::fs::write(&path, text).unwrap();
        path
    }

    fn generate(&self, rows: &[Value], extra: &[&str]) -> (String, Vec<Value>) {
        let input = self.lines(rows);
        let output = Command::new(BIN)
            .arg("generate")
            .arg(&self.project)
            .arg("--requests")
            .arg(&input)
            .args(extra)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let parsed = stdout
            .lines()
            .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{e}: {l}")))
            .collect();
        (stdout, parsed)
    }
}

fn aff(entry: &str, msa: &str, class: &str) -> Value {
    json!({"entryGuid": entry, "msaGuid": msa, "msaClass": class})
}

fn infl() -> Value {
    aff(INFL_ENTRY, INFL_MSA, "MoInflAffMsa")
}

fn deriv() -> Value {
    aff(DERIV_ENTRY, DERIV_MSA, "MoDerivAffMsa")
}

fn request_row(id: &str, msa: &str, morphemes: &[Value]) -> Value {
    json!({"id": id, "op": "generate", "request": {
        "schemaVersion": 1,
        "root": aff(ENTRY, msa, "MoStemMsa"),
        "morphemes": morphemes,
    }})
}

fn all_surfaces(row: &Value) -> Vec<(String, u64)> {
    let mut found = Vec::new();
    for seed in row["seeds"].as_array().unwrap() {
        for s in seed["surfaces"].as_array().unwrap() {
            found.push((
                s["surface"].as_str().unwrap().to_string(),
                s["derivations"].as_u64().unwrap(),
            ));
        }
    }
    found
}

#[test]
fn out_writes_every_row_to_the_file_and_stdout_gets_a_bounded_summary() {
    let run = Run::new();
    let rows: Vec<Value> = (0..300)
        .map(|i| request_row(&format!("r{i}"), MSA, &[infl()]))
        .chain([json!({"id": "bad", "op": "generate"})])
        .collect();
    let (plain_text, plain) = run.generate(&rows, &[]);
    let out = run.dir.path().join("rows.jsonl");
    let (stdout, summary) = run.generate(&rows, &["--out", out.to_str().unwrap()]);
    assert!(
        stdout.lines().count() <= 50 && stdout.len() <= 8192,
        "{stdout}"
    );
    assert_eq!(summary.len(), 1, "{stdout}");
    let summary = &summary[0];
    assert_eq!(summary["type"], "summary");
    assert_eq!(summary["rows"], 301);
    assert_eq!(summary["status"], json!({"ok": 300, "none": 0, "error": 1}));
    assert_eq!(summary["causes"]["bad_request"], 1);
    assert_eq!(summary["complete"], true);
    assert_eq!(summary["out"], out.to_str().unwrap());
    let written = std::fs::read_to_string(&out).unwrap();
    assert_eq!(
        written, plain_text,
        "the file holds exactly the rows stdout held before"
    );
    assert_eq!(plain.len(), 301);
}

#[test]
fn a_capped_run_says_so_and_resumes_from_the_next_line() {
    let run = Run::new();
    let rows: Vec<Value> = (0..10)
        .map(|i| request_row(&format!("r{i}"), MSA, &[infl()]))
        .collect();
    let out = run.dir.path().join("rows.jsonl");
    let out_s = out.to_str().unwrap();
    let out_eq = format!("--out={out_s}");
    let (_, first) = run.generate(&rows, &[out_eq.as_str(), "--max-rows=4"]);
    assert_eq!(first[0]["complete"], false);
    assert_eq!(first[0]["rows"], 4);
    assert_eq!(first[0]["nextLine"], 5);
    assert!(first[0]["resume"]
        .as_str()
        .unwrap()
        .contains("--start-line 5"));
    let (_, rest) = run.generate(&rows, &["--out", out_s, "--append", "--start-line", "5"]);
    assert_eq!(rest[0]["complete"], true);
    let lines: Vec<Value> = std::fs::read_to_string(&out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let numbers: Vec<u64> = lines.iter().map(|r| r["line"].as_u64().unwrap()).collect();
    assert_eq!(numbers, (1..=10).collect::<Vec<_>>());
}

#[test]
fn a_summary_of_ten_thousand_rows_stays_within_fifty_lines_and_eight_kilobytes() {
    let run = Run::new();
    // Rows whose root MSA is unknown are error rows; one per line gives 10,000 rows cheaply.
    let rows: Vec<Value> = (0..10_000)
        .map(|i| {
            request_row(
                &format!("{i}-{}", "x".repeat(300)),
                "00000000-0000-0000-0000-0000000000ff",
                &[],
            )
        })
        .collect();
    let out = run.dir.path().join("rows.jsonl");
    let (stdout, _) = run.generate(&rows, &["--out", out.to_str().unwrap()]);
    assert!(
        stdout.lines().count() <= 50 && stdout.len() <= 8192,
        "{} bytes",
        stdout.len()
    );
    assert_eq!(
        std::fs::read_to_string(&out).unwrap().lines().count(),
        10_000
    );
}

#[test]
fn a_target_without_an_msa_enumerates_the_entrys_msas_and_names_each_one() {
    let run = Run::new();
    let target = json!({"entryGuid": ENTRY});
    let (_, rows) = run.generate(
        &[
            json!({"id": "t", "op": "generate", "target": target, "cell": {"msas": [infl()]}}),
            request_row("a", MSA, &[infl()]),
            request_row("b", MSA2, &[infl()]),
        ],
        &[],
    );
    assert_eq!(rows.len(), 4, "{rows:?}");
    assert_eq!(rows[0]["target"]["msaGuid"], MSA);
    assert_eq!(rows[1]["target"]["msaGuid"], MSA2);
    for (row, expected) in [(&rows[0], &rows[2]), (&rows[1], &rows[3])] {
        assert_eq!(row["status"], "ok", "{row}");
        assert_eq!(row["cause"], Value::Null);
        assert_eq!(row["line"], 1);
        assert_eq!(all_surfaces(row), all_surfaces(expected));
        assert_eq!(
            row["seeds"].as_array().unwrap().len(),
            expected["seeds"].as_array().unwrap().len()
        );
        assert_eq!(row["comboKey"], INFL_MSA);
    }
}

#[test]
fn a_sense_or_an_msa_picks_one_msa_and_a_conflict_is_an_error_row() {
    let run = Run::new();
    let row = |id: &str, target: Value| json!({"id": id, "op": "generate", "target": target, "cell": {"msas": [infl()]}});
    let (_, rows) = run.generate(
        &[
            row("sense", json!({"entryGuid": ENTRY, "senseGuid": SENSE2})),
            row("msa", json!({"entryGuid": ENTRY, "msaGuid": MSA})),
            row(
                "both",
                json!({"entryGuid": ENTRY, "senseGuid": SENSE, "msaGuid": MSA}),
            ),
            row(
                "clash",
                json!({"entryGuid": ENTRY, "senseGuid": SENSE, "msaGuid": MSA2}),
            ),
            row(
                "nosense",
                json!({"entryGuid": ENTRY, "senseGuid": "00000000-0000-0000-0000-0000000000f1"}),
            ),
            row(
                "noentry",
                json!({"entryGuid": "00000000-0000-0000-0000-0000000000f2"}),
            ),
            row(
                "nomsa",
                json!({"entryGuid": ENTRY, "msaGuid": "00000000-0000-0000-0000-0000000000f3"}),
            ),
            row("badshape", json!({"entryGuid": "not-a-guid"})),
            row("affix", json!({"entryGuid": INFL_ENTRY})),
        ],
        &[],
    );
    assert_eq!(rows.len(), 9, "{rows:?}");
    assert_eq!(
        rows[0]["target"],
        json!({"entryGuid": ENTRY, "senseGuid": SENSE2, "msaGuid": MSA2})
    );
    assert_eq!(rows[1]["target"]["msaGuid"], MSA);
    assert_eq!(rows[2]["status"], "ok");
    let errors: Vec<(&str, &str)> = rows[3..8]
        .iter()
        .map(|r| {
            (
                r["id"].as_str().unwrap(),
                r["cause"]["code"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        errors,
        [
            ("clash", "msa_not_owned_by_entry"),
            ("nosense", "unknown_sense"),
            ("noentry", "unknown_entry"),
            ("nomsa", "unknown_msa"),
            ("badshape", "bad_request"),
        ]
    );
    for (i, r) in rows[3..8].iter().enumerate() {
        assert_eq!(r["status"], "error");
        assert_eq!(r["type"], "error");
        assert_eq!(r["line"], i + 4, "error rows keep their line: {r}");
    }
    // An entry with no stem MSA has nothing to generate from: none, not an error.
    assert_eq!(rows[8]["status"], "none", "{}", rows[8]);
    assert_eq!(rows[0]["status"], "ok", "{}", rows[0]);
    assert_eq!(rows[8]["cause"]["code"], "no_stem_msa");
    assert_eq!(rows[8]["line"], 9);
}

#[test]
fn an_empty_result_is_a_none_row_with_a_cause_and_a_trace_by_default() {
    let run = Run::new();
    // A cap with no surface is none with cause cap; maxWork 1 stops before any seed is synthesized.
    let (_, rows) = run.generate(
        &[
            json!({"id": "cap", "op": "generate", "target": {"entryGuid": ENTRY, "msaGuid": MSA},
            "cell": {"msas": [infl()]}, "maxWork": 1}),
        ],
        &[],
    );
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["status"], "none", "{}", rows[0]);
    assert_eq!(rows[0]["cause"]["code"], "cap");
    assert!(rows[0]["cause"]["stopReason"].is_string());
    assert_eq!(rows[0]["complete"], false);
    assert!(
        rows[0]["trace"].is_object(),
        "none rows carry a trace: {}",
        rows[0]
    );
}

#[test]
fn a_cell_of_one_combination_equals_the_request_for_the_same_morphemes() {
    let run = Run::new();
    let (_, rows) = run.generate(
        &[
            json!({"id": "c", "op": "generate", "target": {"entryGuid": ENTRY, "msaGuid": MSA},
                "cell": {"msas": [infl()]}}),
            request_row("r", MSA, &[infl()]),
        ],
        &[],
    );
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0]["cellIndex"], 0);
    assert_eq!(rows[0]["comboKey"], INFL_MSA);
    assert_eq!(all_surfaces(&rows[0]), all_surfaces(&rows[1]));
    assert_eq!(
        all_surfaces(&rows[0])
            .iter()
            .map(|s| s.0.as_str())
            .collect::<Vec<_>>(),
        ["rannas"]
    );
    assert_eq!(
        rows[0]["seeds"][0]["allomorphs"],
        rows[1]["seeds"][0]["allomorphs"]
    );
    assert!(
        rows[0].get("trace").is_none(),
        "an ok row has no trace by default: {}",
        rows[0]
    );
}

#[test]
fn a_cell_mixes_derivational_and_inflectional_affixes_and_flattens_combinations() {
    let run = Run::new();
    let cell = json!({"msas": [[infl()], [deriv(), infl()], [infl(), deriv()]]});
    let (_, rows) = run.generate(
        &[
            json!({"id": "x", "op": "generate", "target": {"entryGuid": ENTRY}, "cell": cell}),
            json!({"id": "y", "op": "generate", "target": {"entryGuid": ENTRY, "msaGuid": MSA}, "cell": cell}),
            request_row("e1", MSA, &[infl()]),
            request_row("e2", MSA, &[deriv(), infl()]),
            request_row("e3", MSA, &[infl(), deriv()]),
        ],
        &[],
    );
    // Two root MSAs times three combinations, then one MSA times three, then three request rows.
    assert_eq!(rows.len(), 12, "{rows:?}");
    let cells: Vec<&Value> = rows[..9].iter().collect();
    assert!(cells[..6].iter().all(|r| r["cellIndex"] == 0));
    assert!(
        cells[6..].iter().all(|r| r["cellIndex"] == 1),
        "each cell has its own index"
    );
    let keys: Vec<&str> = cells[..3]
        .iter()
        .map(|r| r["comboKey"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            INFL_MSA.to_string(),
            format!("{DERIV_MSA}+{INFL_MSA}"),
            format!("{INFL_MSA}+{DERIV_MSA}")
        ]
    );
    for (offset, expected) in [(6, 9), (7, 10), (8, 11)] {
        assert_eq!(
            all_surfaces(&rows[offset]),
            all_surfaces(&rows[expected]),
            "combo {offset}"
        );
        assert_eq!(
            rows[offset]["seeds"].as_array().unwrap().len(),
            rows[expected]["seeds"].as_array().unwrap().len()
        );
        assert_eq!(
            rows[offset]["status"],
            if all_surfaces(&rows[expected]).is_empty() {
                "none"
            } else {
                "ok"
            }
        );
    }
}

#[test]
fn work_is_one_budget_across_combinations_and_a_per_combo_cap_binds_each() {
    let run = Run::new();
    let cell = json!({"msas": [[infl()], [infl()], [infl()]]});
    let target = json!({"entryGuid": ENTRY, "msaGuid": MSA});
    let (_, rows) = run.generate(
        &[
            json!({"id": "free", "op": "generate", "target": target, "cell": cell}),
            json!({"id": "total", "op": "generate", "target": target, "cell": cell, "maxWork": 3}),
            json!({"id": "each", "op": "generate", "target": target, "cell": cell, "maxWorkPerCombo": 1}),
        ],
        &[],
    );
    assert_eq!(rows.len(), 9, "{rows:?}");
    let free_units = rows[0]["workUnits"].as_u64().unwrap();
    assert!(free_units >= 1);
    assert!(rows[..3]
        .iter()
        .all(|r| r["status"] == "ok" && r["complete"] == true));
    let spent: u64 = rows[3..6]
        .iter()
        .map(|r| r["workUnits"].as_u64().unwrap())
        .sum();
    assert!(
        spent <= 3,
        "the three combinations share maxWork 3, used {spent}"
    );
    if free_units * 3 > 3 {
        assert!(
            rows[3..6].iter().any(|r| r["complete"] == false),
            "{:?}",
            &rows[3..6]
        );
        assert!(rows[3..6]
            .iter()
            .any(|r| r["status"] == "none" && r["cause"]["code"] == "cap"));
    }
    assert!(rows[6..]
        .iter()
        .all(|r| r["limits"]["generationCap"].as_u64().unwrap() > 0));
}

#[test]
fn trace_is_on_none_rows_by_default_and_on_every_row_with_trace_all() {
    let run = Run::new();
    let row = json!({"id": "t", "op": "generate", "target": {"entryGuid": ENTRY, "msaGuid": MSA},
        "cell": {"msas": [infl()]}});
    let (_, plain) = run.generate(std::slice::from_ref(&row), &[]);
    let (_, all) = run.generate(&[row], &["--trace-all"]);
    assert!(plain[0].get("trace").is_none());
    assert!(all[0]["trace"].is_object(), "{}", all[0]);
    let surface = &all[0]["seeds"][0]["surfaces"][0];
    assert!(
        surface["steps"].is_array(),
        "per-surface steps ride with the opt-in: {surface}"
    );
    assert_eq!(all_surfaces(&plain[0]), all_surfaces(&all[0]));
}

#[test]
fn bad_cells_and_serve_style_fields_are_error_rows_that_keep_their_line() {
    let run = Run::new();
    let target = json!({"entryGuid": ENTRY, "msaGuid": MSA});
    let rows = [
        json!({"id": "a", "op": "generate", "target": target, "cell": {"features": {"person": "1"}}}),
        json!({"id": "b", "op": "generate", "target": target, "cell": {"msas": []}}),
        json!({"id": "c", "op": "generate", "target": target, "cell": {"msas": [[aff(INFL_ENTRY, "00000000-0000-0000-0000-0000000000f4", "MoInflAffMsa")]]}}),
        json!({"id": "d", "op": "generate", "target": target, "request": {}}),
        json!({"id": "e", "op": "generate", "cell": {"msas": [[]]}}),
    ];
    let (_, out) = run.generate(&rows, &[]);
    assert_eq!(out.len(), 5, "{out:?}");
    for (i, r) in out.iter().enumerate() {
        assert_eq!(r["status"], "error", "{r}");
        assert_eq!(r["line"], i + 1);
    }
    assert_eq!(out[2]["cause"]["code"], "unknown_msa");
}
