use super::*;

/// The dispatch names `main.rs::run` matched before this table existed, pinned here.
const HISTORICAL_DISPATCH_LITERALS: &[&str] = &[
    "batch",
    "generate",
    "parse",
    "import",
    "compare",
    "golden-diff",
    "investigate",
    "fst-health",
    "coverage",
    "plan-diagram",
    "make-report",
    "stats",
    "recipe-optimize",
    "__recipe-optimize-child",
    "__compile-worker-child",
];

#[test]
fn table_covers_every_historical_dispatch_literal() {
    for name in HISTORICAL_DISPATCH_LITERALS {
        assert!(
            find_command(name).is_some(),
            "missing from COMMANDS: {name}"
        );
    }
}

/// Extracts `Some("...")` match-arm literals from `main.rs`'s own source text.
fn some_string_match_literals(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find("Some(\"") {
        let after = &rest[start + "Some(\"".len()..];
        let Some(end) = after.find('"') else { break };
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    out
}

#[test]
fn no_dispatch_literal_in_main_escapes_the_table() {
    let source = include_str!("../main.rs");
    let known_non_command_specials = ["--version", "-V", "--describe"];
    for literal in some_string_match_literals(source) {
        if known_non_command_specials.contains(&literal.as_str()) {
            continue;
        }
        assert!(
            find_command(&literal).is_some(),
            "main.rs matches on \"{literal}\" but it has no COMMANDS row -- the table has \
                 drifted from the real dispatch"
        );
    }
}

#[test]
fn describe_json_lists_batch_with_threads_and_word_timeout_ms() {
    let spec = find_command("batch").expect("batch must be in COMMANDS");
    assert!(spec.flag("--threads").is_some());
    assert!(spec.flag("--word-timeout-ms").is_some());
    assert!(spec.flag("--analyses").is_some());

    let value: serde_json::Value = serde_json::to_value(&Describe {
        schema_version: 1,
        binary: "pangloss",
        commands: COMMANDS,
    })
    .expect("Describe must serialize");
    let batch = value["commands"]
        .as_array()
        .expect("commands must be an array")
        .iter()
        .find(|c| c["name"] == "batch")
        .expect("batch must be listed");
    let flag_names: Vec<&str> = batch["flags"]
        .as_array()
        .expect("flags must be an array")
        .iter()
        .map(|f| f["name"].as_str().expect("flag name must be a string"))
        .collect();
    assert!(flag_names.contains(&"--threads"));
    assert!(flag_names.contains(&"--word-timeout-ms"));
    assert!(flag_names.contains(&"--analyses"));
}

#[test]
fn describe_json_step_cap_flag_documents_default_and_unbounded() {
    let spec = find_command("batch").expect("batch must be in COMMANDS");
    let flag = spec.flag("--step-cap").expect("--step-cap must be listed");
    assert!(flag.takes_value);
    assert!(flag.summary.contains("50000000"), "{}", flag.summary);
    assert!(flag.summary.contains("unbounded"), "{}", flag.summary);
}

#[test]
fn hidden_commands_are_marked_hidden_in_the_described_json() {
    let value: serde_json::Value = serde_json::to_value(&Describe {
        schema_version: 1,
        binary: "pangloss",
        commands: COMMANDS,
    })
    .expect("Describe must serialize");
    for row in value["commands"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let expected_hidden = name.starts_with("__");
        assert_eq!(
            row["hidden"], expected_hidden,
            "{name}: hidden flag in the JSON must match its COMMANDS row"
        );
    }
}

#[test]
fn every_command_flag_starts_with_double_dash() {
    for command in COMMANDS {
        for flag in command.flags {
            assert!(
                flag.name.starts_with("--"),
                "{}: flag {:?} must be spelled with a leading --",
                command.name,
                flag.name
            );
        }
    }
}

#[test]
fn make_report_allow_unproven_flag_is_cfg_gated_the_same_way_as_the_parser() {
    let spec = find_command("make-report").expect("make-report must be in COMMANDS");
    let declared = spec.flag("--allow-unproven").is_some();
    let compiled_in = cfg!(feature = "developer-tools");
    assert_eq!(
        declared, compiled_in,
        "--allow-unproven must be declared in the spec exactly when developer-tools is enabled"
    );
}
