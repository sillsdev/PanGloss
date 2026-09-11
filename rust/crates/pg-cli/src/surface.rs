//! The dispatch table `main.rs::run` looks up by name, and the source `pangloss --describe` renders as JSON.

use std::process::ExitCode;

use serde::Serialize;

/// One flag a subcommand's hand-rolled parser recognizes.
#[derive(Serialize)]
pub(crate) struct FlagSpec {
    pub(crate) name: &'static str,
    pub(crate) takes_value: bool,
    pub(crate) summary: &'static str,
}

/// One subcommand row: identity, help text, visibility, and its declared positional/flag surface.
#[derive(Serialize)]
pub(crate) struct CommandSpec {
    pub(crate) name: &'static str,
    pub(crate) summary: &'static str,
    pub(crate) hidden: bool,
    pub(crate) positionals: &'static [&'static str],
    pub(crate) flags: &'static [FlagSpec],
    /// Not part of the described surface -- the function `run()` actually calls for this row.
    #[serde(skip)]
    pub(crate) handler: fn(&[String]) -> ExitCode,
}

impl CommandSpec {
    pub(crate) fn flag(&self, name: &str) -> Option<&FlagSpec> {
        self.flags.iter().find(|f| f.name == name)
    }

    /// A positional always passes; a `--flag` absent from this row's own `flags` is rejected.
    pub(crate) fn reject_unknown_option(&self, arg: &str) -> Result<(), String> {
        let Some(rest) = arg.strip_prefix("--") else {
            return Ok(());
        };
        let name = format!("--{}", rest.split('=').next().unwrap_or(rest));
        if self.flag(&name).is_some() {
            // Declared but unmatched above -- a value-syntax mistake, not an unknown flag name.
            return Err(format!(
                "option {name} is declared for `{}` but `{arg}` could not be parsed (check its \
                 expected value syntax)",
                self.name
            ));
        }
        Err(format!("unknown option: {arg}"))
    }
}

pub(crate) fn find_command(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|c| c.name == name)
}

fn dispatch(
    name: &'static str,
    args: &[String],
    f: fn(&[String]) -> Result<(), String>,
) -> ExitCode {
    match f(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pangloss {name}: {e}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch_batch(args: &[String]) -> ExitCode {
    dispatch("batch", args, crate::run_batch)
}
fn dispatch_generate(args: &[String]) -> ExitCode {
    dispatch("generate", args, crate::run_generate)
}
fn dispatch_parse(args: &[String]) -> ExitCode {
    dispatch("parse", args, crate::run_parse)
}
fn dispatch_import(args: &[String]) -> ExitCode {
    dispatch("import", args, crate::run_import)
}
fn dispatch_fst_health(args: &[String]) -> ExitCode {
    dispatch("fst-health", args, crate::fst_health::run_fst_health)
}
fn dispatch_coverage(args: &[String]) -> ExitCode {
    dispatch("coverage", args, crate::coverage::run_coverage)
}
fn dispatch_plan_diagram(args: &[String]) -> ExitCode {
    dispatch("plan-diagram", args, crate::plan_diagram::run_plan_diagram)
}
fn dispatch_make_report(args: &[String]) -> ExitCode {
    dispatch("make-report", args, crate::make_report::run_make_report)
}
fn dispatch_stats(args: &[String]) -> ExitCode {
    dispatch("stats", args, crate::stats_cmd::run_stats)
}
fn dispatch_recipe_optimize(args: &[String]) -> ExitCode {
    match crate::recipe_optimize::run_recipe_optimize(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pangloss recipe-optimize: {e}");
            ExitCode::FAILURE
        }
    }
}
fn dispatch_recipe_optimize_child(args: &[String]) -> ExitCode {
    match crate::recipe_optimize::run_recipe_optimize(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pangloss __recipe-optimize-child: {e}");
            ExitCode::FAILURE
        }
    }
}
fn dispatch_compare(args: &[String]) -> ExitCode {
    crate::assess::exit(crate::assess::run_compare(args), "compare")
}
fn dispatch_golden_diff(args: &[String]) -> ExitCode {
    crate::assess::exit(crate::assess::run_golden_diff(args), "golden-diff")
}
fn dispatch_investigate(args: &[String]) -> ExitCode {
    crate::assess::exit(crate::assess::run_investigate(args), "investigate")
}
fn dispatch_compile_worker_child(_args: &[String]) -> ExitCode {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match pg_foma::worker::run_worker_child(stdin.lock(), stdout.lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pangloss __compile-worker-child: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The top-level `{ schema_version, binary, commands }` document `pangloss --describe` prints.
#[derive(Serialize)]
struct Describe {
    schema_version: u32,
    binary: &'static str,
    commands: &'static [CommandSpec],
}

/// Prints `COMMANDS` as JSON, hidden rows marked `"hidden": true` rather than omitted.
pub(crate) fn run_describe(_args: &[String]) -> ExitCode {
    let doc = Describe {
        schema_version: 1,
        binary: "pangloss",
        commands: COMMANDS,
    };
    match serde_json::to_string_pretty(&doc) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("pangloss describe: serialize surface: {e}");
            ExitCode::FAILURE
        }
    }
}

const BATCH_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        name: "--step-cap",
        takes_value: true,
        summary: "N or \"unbounded\"; bound the unmemoized analysis cascade's step count per word (default: 50000000, a runaway guard)",
    },
    FlagSpec {
        name: "--word-timeout-ms",
        takes_value: true,
        summary: "wall-clock deadline per word, independent of --step-cap",
    },
    FlagSpec {
        name: "--memo",
        takes_value: true,
        summary: "on|off; enable/disable analysis memoization (default on)",
    },
    FlagSpec {
        name: "--threads",
        takes_value: true,
        summary: "thread count; 1 keeps the crash-resumable sequential writer",
    },
    FlagSpec {
        name: "--start",
        takes_value: true,
        summary: "0-based resume index; skips already-completed words and appends to <out.tsv>",
    },
    FlagSpec {
        name: "--analyses",
        takes_value: true,
        summary: "write FieldWorks ParseAnalysis JSONL rows to this sidecar path",
    },
    FlagSpec {
        name: "--guess",
        takes_value: false,
        summary: "retry an out-of-lexicon empty analysis via the lexical-pattern guesser",
    },
    FlagSpec {
        name: "--stats",
        takes_value: false,
        summary: "additionally record per-object statistics into the stats cache",
    },
    FlagSpec {
        name: "--cache",
        takes_value: true,
        summary: "stats cache path override (with --stats)",
    },
    FlagSpec {
        name: "--always-enforce-final-templates",
        takes_value: false,
        summary: "enforce template order even where partial-rule rescue would allow interleaving",
    },
];

const GENERATE_FLAGS: &[FlagSpec] = &[];

const PARSE_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        name: "--trace",
        takes_value: true,
        summary: "trace to stdout, or to a file with --trace=<file>",
    },
    FlagSpec {
        name: "--trace-format",
        takes_value: true,
        summary: "text|json (default text)",
    },
    FlagSpec {
        name: "--gloss",
        takes_value: false,
        summary: "print a Leipzig gloss line per analysis",
    },
    FlagSpec {
        name: "--natural-gloss",
        takes_value: true,
        summary: "eng; print a natural-language realization line per analysis",
    },
    FlagSpec {
        name: "--realize-map",
        takes_value: true,
        summary: "explicit --natural-gloss sidecar map path override",
    },
    FlagSpec {
        name: "--guess",
        takes_value: false,
        summary: "retry an out-of-lexicon empty analysis via the lexical-pattern guesser",
    },
];

const IMPORT_FLAGS: &[FlagSpec] = &[];

const COMPARE_FLAGS: &[FlagSpec] = &[FlagSpec {
    name: "--report",
    takes_value: true,
    summary: "write the delta artifact here instead of stdout",
}];

const GOLDEN_DIFF_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        name: "--suite",
        takes_value: true,
        summary: "required; the golden suite JSON to diff the report against",
    },
    FlagSpec {
        name: "--report",
        takes_value: true,
        summary: "write the diff artifact here instead of stdout",
    },
];

const INVESTIGATE_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        name: "--case",
        takes_value: true,
        summary: "required; the case id to produce a handoff for",
    },
    FlagSpec {
        name: "--report",
        takes_value: true,
        summary: "write the handoff artifact here instead of stdout",
    },
];

const FST_HEALTH_FLAGS: &[FlagSpec] = &[];

const COVERAGE_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        name: "--json",
        takes_value: false,
        summary: "print canonical JSON to stdout instead of the human-readable summary",
    },
    FlagSpec {
        name: "--grammar",
        takes_value: true,
        summary: "also compute plan-node interaction coverage for this grammar",
    },
];

const PLAN_DIAGRAM_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        name: "--json",
        takes_value: false,
        summary: "print the plan document JSON instead of a mermaid diagram",
    },
    FlagSpec {
        name: "--full",
        takes_value: false,
        summary:
            "render every node, no sibling-leaf collapsing (mutually exclusive with --threshold)",
    },
    FlagSpec {
        name: "--threshold",
        takes_value: true,
        summary: "sibling-leaf collapse threshold (mutually exclusive with --full)",
    },
];

#[cfg(feature = "developer-tools")]
const MAKE_REPORT_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        name: "--pack",
        takes_value: true,
        summary: "the built .pgpack artifact to report on",
    },
    FlagSpec {
        name: "--policy",
        takes_value: true,
        summary: "readiness threshold policy JSON override (default: policy_v1)",
    },
    FlagSpec {
        name: "--allow-unproven",
        takes_value: false,
        summary: "developer-tools only; force-compile and measure a capability-refused grammar",
    },
];
#[cfg(not(feature = "developer-tools"))]
const MAKE_REPORT_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        name: "--pack",
        takes_value: true,
        summary: "the built .pgpack artifact to report on",
    },
    FlagSpec {
        name: "--policy",
        takes_value: true,
        summary: "readiness threshold policy JSON override (default: policy_v1)",
    },
];

const STATS_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        name: "--group",
        takes_value: true,
        summary: "word|object|allomorph|morpheme|group|never-fires (default: object)",
    },
    FlagSpec {
        name: "--kind",
        takes_value: true,
        summary: "restrict to one ObjectKind",
    },
    FlagSpec {
        name: "--object",
        takes_value: true,
        summary: "restrict to one object's structural key",
    },
    FlagSpec {
        name: "--stratum",
        takes_value: true,
        summary: "restrict to one stratum's structural key",
    },
    FlagSpec {
        name: "--direction",
        takes_value: true,
        summary: "analysis|synthesis",
    },
    FlagSpec {
        name: "--word",
        takes_value: true,
        summary: "narrow to one word's own fact rows",
    },
    FlagSpec {
        name: "--top",
        takes_value: true,
        summary: "top N rows per kind after totals are computed",
    },
    FlagSpec {
        name: "--sort",
        takes_value: true,
        summary: "time|no-root|amp|uses|attempts (only for --group object)",
    },
    FlagSpec {
        name: "--exclude-censored",
        takes_value: false,
        summary: "exclude words whose run was capped/timed out/invalid-shape",
    },
    FlagSpec {
        name: "--wide",
        takes_value: false,
        summary: "append work/not_applied/no_root/surface_mismatch/identity_quality columns",
    },
    FlagSpec {
        name: "--by-kind",
        takes_value: false,
        summary: "one section per kind, each with its share of the run's total time",
    },
    FlagSpec {
        name: "--format",
        takes_value: true,
        summary: "text|jsonl (default text)",
    },
    FlagSpec {
        name: "--cache",
        takes_value: true,
        summary: "stats cache path override",
    },
    FlagSpec {
        name: "--out",
        takes_value: true,
        summary: "write the report here instead of stdout",
    },
];

const RECIPE_OPTIMIZE_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        name: "--seed",
        takes_value: true,
        summary: "deterministic search seed",
    },
    FlagSpec {
        name: "--candidates",
        takes_value: true,
        summary: "Budget::candidates",
    },
    FlagSpec {
        name: "--evaluations",
        takes_value: true,
        summary: "Budget::evaluations",
    },
    FlagSpec {
        name: "--elapsed-ns",
        takes_value: true,
        summary: "Budget::elapsed",
    },
    FlagSpec {
        name: "--build-ns",
        takes_value: true,
        summary: "Budget::build",
    },
    FlagSpec {
        name: "--memory-bytes",
        takes_value: true,
        summary: "Budget::memory; also the supervisor's kill threshold",
    },
    FlagSpec {
        name: "--confirmation-work",
        takes_value: true,
        summary: "Budget::confirmation; a call count, not nanoseconds",
    },
    FlagSpec {
        name: "--reserve-ns",
        takes_value: true,
        summary: "Budget::reserve; must not exceed --elapsed-ns",
    },
    FlagSpec {
        name: "--oracle-step-cap",
        takes_value: true,
        summary: "RuntimeBudget::oracle_step_cap override (default: RuntimeBudget's own default)",
    },
    FlagSpec {
        name: "--oracle-liveness-net-ms",
        takes_value: true,
        summary: "aborts the run (never excludes a word) if a word runs past this deadline",
    },
    FlagSpec {
        name: "--oracle-word-timeout-ms",
        takes_value: true,
        summary: "legacy alias for --oracle-liveness-net-ms",
    },
    FlagSpec {
        name: "--oracle-memory-ceiling-bytes",
        takes_value: true,
        summary: "aborts the run if exceeded; never classifies a word",
    },
    FlagSpec {
        name: "--search-all-families",
        takes_value: false,
        summary: "search every backend family regardless of the compositional-topology heuristic",
    },
];

pub(crate) const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "batch",
        summary: "Parse every word in a word list against a grammar and write a batch TSV.",
        hidden: false,
        positionals: &["grammar", "words.txt", "out.tsv"],
        flags: BATCH_FLAGS,
        handler: dispatch_batch,
    },
    CommandSpec {
        name: "generate",
        summary: "Generate a surface word form for a root morpheme and a chain of other morphemes.",
        hidden: false,
        positionals: &["grammar", "root-morpheme-id", "other-morpheme-id..."],
        flags: GENERATE_FLAGS,
        handler: dispatch_generate,
    },
    CommandSpec {
        name: "parse",
        summary: "Parse, and optionally trace/gloss/realize, a single word against a grammar.",
        hidden: false,
        positionals: &["grammar", "word"],
        flags: PARSE_FLAGS,
        handler: dispatch_parse,
    },
    CommandSpec {
        name: "import",
        summary: "Import a FieldWorks .fwdata project into a pg-snapshot JSON file.",
        hidden: false,
        positionals: &["project.fwdata", "out.json"],
        flags: IMPORT_FLAGS,
        handler: dispatch_import,
    },
    CommandSpec {
        name: "compare",
        summary: "Diff a baseline assessment report against a candidate report.",
        hidden: false,
        positionals: &["baseline.json", "candidate.json"],
        flags: COMPARE_FLAGS,
        handler: dispatch_compare,
    },
    CommandSpec {
        name: "golden-diff",
        summary: "Diff an assessment report against a golden suite.",
        hidden: false,
        positionals: &["report.json"],
        flags: GOLDEN_DIFF_FLAGS,
        handler: dispatch_golden_diff,
    },
    CommandSpec {
        name: "investigate",
        summary: "Produce an investigation handoff for one case in an assessment report.",
        hidden: false,
        positionals: &["report.json"],
        flags: INVESTIGATE_FLAGS,
        handler: dispatch_investigate,
    },
    CommandSpec {
        name: "fst-health",
        summary: "Run grammar-only FST characterization and write a HealthReport.",
        hidden: false,
        positionals: &["grammar", "out.json?"],
        flags: FST_HEALTH_FLAGS,
        handler: dispatch_fst_health,
    },
    CommandSpec {
        name: "coverage",
        summary:
            "Report capability/conformance coverage over the registered characteristic ledger.",
        hidden: false,
        positionals: &["out.json?"],
        flags: COVERAGE_FLAGS,
        handler: dispatch_coverage,
    },
    CommandSpec {
        name: "plan-diagram",
        summary: "Render a grammar's compiled Plan as JSON or a mermaid diagram.",
        hidden: false,
        positionals: &["grammar", "out?"],
        flags: PLAN_DIAGRAM_FLAGS,
        handler: dispatch_plan_diagram,
    },
    CommandSpec {
        name: "make-report",
        summary: "Compose a markdown readiness report from an already-built artifact.",
        hidden: false,
        positionals: &["grammar", "out.md"],
        flags: MAKE_REPORT_FLAGS,
        handler: dispatch_make_report,
    },
    CommandSpec {
        name: "stats",
        summary: "Report per-project HC/Foma engine statistics from the stats cache.",
        hidden: false,
        positionals: &["project-or-grammar"],
        flags: STATS_FLAGS,
        handler: dispatch_stats,
    },
    CommandSpec {
        name: "recipe-optimize",
        summary: "Search backend recipe candidates for a grammar under a fixed evidence budget.",
        hidden: false,
        positionals: &["grammar", "words.txt", "out-dir"],
        flags: RECIPE_OPTIMIZE_FLAGS,
        handler: dispatch_recipe_optimize,
    },
    CommandSpec {
        name: "describe",
        summary: "Print this CLI's subcommand/flag surface as machine-readable JSON.",
        hidden: false,
        positionals: &[],
        flags: &[],
        handler: run_describe,
    },
    CommandSpec {
        name: "__recipe-optimize-child",
        summary: "Internal recipe-optimize worker process; not for direct use.",
        hidden: true,
        positionals: &["grammar", "words.txt", "out-dir"],
        flags: RECIPE_OPTIMIZE_FLAGS,
        handler: dispatch_recipe_optimize_child,
    },
    CommandSpec {
        name: "__compile-worker-child",
        summary: "Internal out-of-process foma compile worker; not for direct use.",
        hidden: true,
        positionals: &[],
        flags: &[],
        handler: dispatch_compile_worker_child,
    },
];

#[cfg(test)]
mod tests {
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
        let source = include_str!("main.rs");
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
}
