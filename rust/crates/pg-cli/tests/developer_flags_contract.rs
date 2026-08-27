use std::process::Command;

const DEVELOPER_FLAGS: [&str; 1] = ["--allow-unproven"];
const REMOVED_FLAGS: [&str; 2] = ["--remove-size-limits", "--no-enforce-capability"];

fn pangloss(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args(args)
        .output()
        .expect("pangloss must start")
}

fn combined_output(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[cfg(not(feature = "developer-tools"))]
#[test]
fn production_help_omits_developer_only_flags() {
    let output = pangloss(&[]);
    let text = combined_output(&output);

    for flag in DEVELOPER_FLAGS {
        assert!(
            !text.contains(flag),
            "production help must omit developer-only flag {flag}: {text}"
        );
    }
    for flag in REMOVED_FLAGS {
        assert!(!text.contains(flag), "production help must omit removed flag {flag}: {text}");
    }
}

#[cfg(not(feature = "developer-tools"))]
#[test]
fn production_commands_reject_developer_flags_as_unknown_options() {
    for flag in DEVELOPER_FLAGS.into_iter().chain(REMOVED_FLAGS) {
        for args in [
            vec!["parse", "missing.xml", "word", flag],
            vec!["batch", "missing.xml", "words.txt", "out.tsv", flag],
            vec!["make-report", "missing.xml", "out.md", flag],
        ] {
            let output = pangloss(&args);
            let text = combined_output(&output);
            assert!(!output.status.success(), "{args:?} unexpectedly succeeded");
            assert!(
                text.contains("unknown option"),
                "{args:?} must reject {flag} before positional fallback: {text}"
            );
        }
    }
}

#[cfg(feature = "developer-tools")]
#[test]
fn developer_build_accepts_flags_without_building_a_grammar() {
    for args in [
        vec!["parse", "missing.xml", "word", "--allow-unproven"],
        vec!["batch", "missing.xml", "words.txt", "out.tsv", "--allow-unproven"],
        vec!["make-report", "missing.xml", "out.md", "--allow-unproven"],
    ] {
        let output = pangloss(&args);
        let text = combined_output(&output);
        assert!(
            !text.contains("unknown option"),
            "developer build must parse {:?} before grammar loading: {text}",
            args
        );
    }
}

#[cfg(feature = "developer-tools")]
#[test]
fn developer_build_rejects_removed_flags_on_all_commands() {
    for flag in REMOVED_FLAGS {
        for args in [
            vec!["parse", "missing.xml", "word", flag],
            vec!["batch", "missing.xml", "words.txt", "out.tsv", flag],
            vec!["make-report", "missing.xml", "out.md", flag],
        ] {
            let output = pangloss(&args);
            let text = combined_output(&output);
            assert!(!output.status.success(), "{args:?} unexpectedly succeeded");
            assert!(
                text.contains("unknown option"),
                "developer build must reject removed flag {:?} on every command: {text}",
                args
            );
        }
    }
}

#[cfg(feature = "developer-tools")]
#[test]
fn developer_help_mentions_only_the_remaining_developer_flag() {
    let output = pangloss(&[]);
    let text = combined_output(&output);

    for flag in DEVELOPER_FLAGS {
        assert!(
            text.contains(flag),
            "developer help must mention {flag}: {text}"
        );
    }
    for flag in REMOVED_FLAGS {
        assert!(
            !text.contains(flag),
            "developer help must omit removed flag {flag}: {text}"
        );
    }
}
