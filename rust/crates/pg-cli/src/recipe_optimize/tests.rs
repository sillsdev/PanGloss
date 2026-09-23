use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{parse_args, read_progress_rows, RecipeOptimizeError};

/// Drives `parse_args` directly rather than scraping its source for matching literals.
mod flag_spec_drives_the_parser {
    use crate::recipe_optimize::{parse_args, RecipeOptimizeError};
    use crate::surface::find_command;

    fn base_args() -> Vec<String> {
        vec!["grammar.xml".into(), "words.txt".into(), "out".into()]
    }

    #[test]
    fn unknown_flag_is_rejected_with_the_shared_message() {
        let mut args = base_args();
        args.push("--bogus-flag".into());
        args.push("1".into());
        let error = parse_args(&args).expect_err("an undeclared flag must be rejected");
        assert_eq!(
            error,
            RecipeOptimizeError::Usage("unknown option: --bogus-flag".to_string())
        );
    }

    /// Drives `parse_args` with each declared flag in turn, so a value-taking flag gets a value.
    #[test]
    fn every_declared_flag_is_accepted_by_parse_args() {
        let spec = find_command("recipe-optimize").expect("recipe-optimize must be in COMMANDS");
        for flag in spec.flags {
            let mut args = base_args();
            args.push(flag.name.to_string());
            if flag.takes_value {
                args.push("1".to_string());
            }
            assert!(
                parse_args(&args).is_ok(),
                "{}: expected parse_args to accept this declared flag",
                flag.name
            );
        }
    }
}

#[test]
fn usage_documents_search_all_families_replay_flag() {
    let error = parse_args(&["grammar.xml".into(), "words.txt".into()]).unwrap_err();
    match error {
        RecipeOptimizeError::Usage(message) => {
            assert!(message.contains("--search-all-families"));
        }
        other => panic!("expected usage error, got {other:?}"),
    }
}

#[test]
fn search_all_families_parses_as_replay_opt_in() {
    let args = vec![
        "grammar.xml".into(),
        "words.txt".into(),
        "out".into(),
        "--search-all-families".into(),
    ];
    assert!(parse_args(&args).unwrap().search_all_families);
}

#[test]
fn progress_reader_keeps_complete_rows_and_discards_malformed_or_truncated_rows() {
    let tag = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("pangloss-recipe-progress-{tag}.jsonl"));
    let complete = serde_json::json!({
        "id": "candidate-1",
        "backend_id": "backend-1",
        "certification": {
            "status": "full-hc-confirmed",
            "words": 1,
            "corpus_hash": "hash"
        },
        "score": {
            "states": 1,
            "arcs": 1,
            "build": 1,
            "apply": 1,
            "proposals": 1,
            "confirmation": 1,
            "confirmation_steps": 1,
            "raw_paths": 1
        },
        "realized_strategy": "plan-composed"
    });
    fs::write(
        &path,
        format!("{}\nnot-json\n{{\"id\":\"truncated\"", complete),
    )
    .unwrap();

    let rows = read_progress_rows(&path);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].report.id, "candidate-1");
    assert_eq!(rows[0].realized_strategy.as_str(), "plan-composed");
    let _ = fs::remove_file(path);
}
