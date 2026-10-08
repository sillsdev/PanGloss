use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pg_facts::{FactsError, ProducerIdentity, StatsInput};

pub(crate) fn run_facts(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args) {
        Ok(parsed) => parsed,
        Err(error) => return fail(error.code(), &error.to_string()),
    };
    if parsed.snapshot.extension().and_then(|value| value.to_str()) != Some("json") {
        return fail(
            "unsupported_facts_source",
            "facts accepts only current pg-snapshot JSON input",
        );
    }
    let input_paths = [
        Some(&parsed.snapshot),
        Some(&parsed.context),
        parsed.stats_cache.as_ref(),
        parsed.stats_manifest.as_ref(),
    ];
    let paths_alias = input_paths.iter().enumerate().any(|(left_index, left)| {
        left.is_some_and(|left| {
            input_paths
                .iter()
                .skip(left_index + 1)
                .flatten()
                .any(|right| same_path(left, right))
                || same_path(left, &parsed.output)
        })
    });
    if paths_alias {
        return fail(
            "input_output_collision",
            "Snapshot, context, stats cache, manifest, and output paths must not resolve to the same file",
        );
    }
    let source = match std::fs::read(parsed.snapshot) {
        Ok(bytes) => bytes,
        Err(error) => return fail("facts_write_failed", &error.to_string()),
    };
    let context = match std::fs::read(parsed.context) {
        Ok(bytes) => bytes,
        Err(error) => return fail("facts_write_failed", &error.to_string()),
    };
    let producer = ProducerIdentity {
        compiler_version: env!("CARGO_PKG_VERSION"),
        source_revision: env!("PANGLOSS_BUILD_REVISION"),
        build_identity: crate::build_info::embedded_build_info(),
    };
    let write_result = match (&parsed.stats_cache, &parsed.stats_manifest) {
        (Some(cache_path), Some(manifest_path)) => pg_facts::write_facts_with_stats(
            &source,
            &context,
            &parsed.output,
            producer,
            StatsInput::new(cache_path, manifest_path),
        ),
        (None, None) => pg_facts::write_facts(&source, &context, &parsed.output, producer),
        _ => unreachable!("parse_args requires --stats and --stats-manifest together"),
    };
    let result = match write_result {
        Ok(result) => result,
        Err(error) => return fail(error.code(), &error.to_string()),
    };
    if parsed.json {
        match serde_json::to_string_pretty(&result) {
            Ok(json) => println!("{json}"),
            Err(error) => return fail("facts_write_failed", &error.to_string()),
        }
    } else {
        println!(
            "wrote {} ({} bytes, {}, schema {}, compile {})",
            parsed.output.display(),
            result.output_bytes,
            result.output_sha256,
            result.schema_version,
            result.compile_status
        );
    }
    if result.compile_status == "refused" {
        eprintln!(
            "pangloss facts: compile_refused: authored and diagnostic facts were published; effective grammar is unavailable"
        );
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

struct FactsArgs {
    snapshot: PathBuf,
    output: PathBuf,
    context: PathBuf,
    stats_cache: Option<PathBuf>,
    stats_manifest: Option<PathBuf>,
    json: bool,
}

fn parse_args(args: &[String]) -> Result<FactsArgs, FactsError> {
    let Some(snapshot) = args.first() else {
        return Err(FactsError::InvalidContext(
            "usage: facts <snapshot.json> --out <facts.sqlite> --context <context.json> [--stats <cache.sqlite> --stats-manifest <manifest.json>] [--json]"
                .into(),
        ));
    };
    let mut output = None;
    let mut context = None;
    let mut stats_cache = None;
    let mut stats_manifest = None;
    let mut json = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--out" => {
                index += 1;
                output = Some(PathBuf::from(args.get(index).ok_or_else(|| {
                    FactsError::InvalidContext("--out requires a path".into())
                })?));
            }
            "--context" => {
                index += 1;
                context = Some(PathBuf::from(args.get(index).ok_or_else(|| {
                    FactsError::InvalidContext("--context requires a path".into())
                })?));
            }
            "--stats" => {
                index += 1;
                stats_cache = Some(PathBuf::from(args.get(index).ok_or_else(|| {
                    FactsError::InvalidContext("--stats requires a cache path".into())
                })?));
            }
            "--stats-manifest" => {
                index += 1;
                stats_manifest = Some(PathBuf::from(args.get(index).ok_or_else(|| {
                    FactsError::InvalidContext("--stats-manifest requires a manifest path".into())
                })?));
            }
            "--json" => json = true,
            unknown => {
                return Err(FactsError::InvalidContext(format!(
                    "unknown facts argument {unknown:?}"
                )));
            }
        }
        index += 1;
    }
    let output = output.ok_or_else(|| FactsError::InvalidContext("--out is required".into()))?;
    let context =
        context.ok_or_else(|| FactsError::InvalidContext("--context is required".into()))?;
    if stats_cache.is_some() != stats_manifest.is_some() {
        return Err(FactsError::InvalidContext(
            "--stats and --stats-manifest must be supplied together".into(),
        ));
    }
    Ok(FactsArgs {
        snapshot: PathBuf::from(snapshot),
        output,
        context,
        stats_cache,
        stats_manifest,
        json,
    })
}

fn same_path(input: &Path, output: &Path) -> bool {
    normalized_path(input)
        .zip(normalized_path(output))
        .is_some_and(|(a, b)| a == b)
}

fn normalized_path(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return std::fs::canonicalize(path).ok();
    }
    let parent = path.parent().filter(|path| !path.as_os_str().is_empty())?;
    let name = path.file_name()?;
    Some(std::fs::canonicalize(parent).ok()?.join(name))
}

fn fail(code: &str, message: &str) -> ExitCode {
    eprintln!("pangloss facts: {code}: {message}");
    ExitCode::FAILURE
}
