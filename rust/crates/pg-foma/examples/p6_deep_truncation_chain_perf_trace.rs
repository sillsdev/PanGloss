//! Runtime trace for the real Aweti grammar: a 50,000-path allowance shared across a word's direct and reduplication-peel proposals; a cap trip prints `UNMEASURED` and never reaches confirmation.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pg_foma::compose_budget::ApplyBudget;
use pg_foma::composite::{FomaAnalyzer, ProfiledFomaApplyOutcome};
use pg_foma::templated_compile::{compile_templated_morphotactics, TemplatedCompileOutput};
use pg_grammar::model::Grammar;

const APPLY_PATH_CAP: usize = 50_000;
const STACK_BYTES: usize = 512 * 1024 * 1024;

fn sample_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/data")
        .join(name)
}

fn load_aweti() -> Grammar {
    let path = sample_path("aweti.json");
    let json =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let snapshot = pg_snapshot::Snapshot::from_json(&json)
        .unwrap_or_else(|e| panic!("parse snapshot {}: {e}", path.display()));
    let (grammar, warnings) = pg_grammar::compile_project(&snapshot)
        .unwrap_or_else(|e| panic!("compile_project {}: {e}", path.display()));
    if !warnings.is_empty() {
        eprintln!("compile_project_warnings={}", warnings.len());
    }
    grammar
}

fn main() -> ExitCode {
    std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run)
        .expect("spawn large-stack Aweti trace worker")
        .join()
        .expect("Aweti trace worker panicked")
}

fn run() -> ExitCode {
    let mut words: Vec<String> = std::env::args().skip(1).collect();
    if words.is_empty() {
        words = ["parua", "an", "ti"]
            .into_iter()
            .map(str::to_owned)
            .collect();
    }
    if words.iter().any(|word| word == "tomoʼatu") {
        eprintln!(
            "UNMEASURED word=tomoʼatu reason=oracle-hazard \
             use the capped p6_templated_q3_oracle_bounds example; this trace refuses that probe"
        );
        return ExitCode::from(2);
    }

    let grammar = load_aweti();
    let compiled = match compile_templated_morphotactics(&grammar) {
        Ok(compiled) => compiled,
        Err(error) => {
            eprintln!("UNMEASURED stage=p6-templated-compile reason=unsupported error={error}");
            return ExitCode::from(2);
        }
    };
    let TemplatedCompileOutput {
        network: _,
        proposer,
        profile,
    } = compiled;
    println!(
        "COMPILE states={} arcs={} lexc_states={} lexc_arcs={} rules={} skipped_rules={} \
         tuple_report_rules={} lexc_lines={}",
        profile.final_state_count,
        profile.final_arc_count,
        profile.lexc_state_count,
        profile.lexc_arc_count,
        profile.phonological_rule_count,
        profile.skipped_rules.len(),
        profile.tuple_reports.len(),
        proposer.report.counts.lexc_lines,
    );
    for (stage, elapsed) in [
        ("templated_emit", profile.templated_emit_elapsed),
        ("lexc_compile", profile.lexc_compile_elapsed),
        ("rule_compile_compose", profile.rule_compile_compose_elapsed),
        ("cleanup_compile", profile.cleanup_compile_elapsed),
        (
            "final_compose_minimize",
            profile.final_compose_minimize_elapsed,
        ),
        ("apply_prepare", profile.apply_prepare_elapsed),
    ] {
        println!(
            "COMPILE_STAGE stage={stage} elapsed_ms={:.3}",
            elapsed.as_secs_f64() * 1_000.0
        );
    }

    let mut analyzer = FomaAnalyzer::from_precompiled_proposer(&grammar, proposer);
    let budget = ApplyBudget::with_caps(Some(APPLY_PATH_CAP), None);
    for word in words {
        match analyzer.analyze_word_with_diagnostics_budgeted(&word, &budget) {
            ProfiledFomaApplyOutcome::Complete(profiled) => {
                let d = &profiled.diagnostics;
                println!(
                    "MEASURED word={word:?} raw_paths={} raw_bytes={} decoded_paths={} \
                     malformed_paths={} proposal_unique_candidates={} final_candidates={} \
                     proposal_calls={} traversal_ms={:.3} decode_dedup_ms={:.3} \
                     confirm_batch_calls={} confirmation_groups={} confirmation_calls={} \
                     confirmation_ms={:.3} confirmed={}",
                    d.proposal.raw_paths,
                    d.proposal.raw_bytes,
                    d.proposal.decoded_paths,
                    d.proposal.malformed_paths,
                    d.proposal.unique_candidates,
                    profiled.outcome.candidates_generated,
                    d.proposal_calls,
                    d.proposal.traversal_elapsed.as_secs_f64() * 1_000.0,
                    d.proposal.decode_dedup_elapsed.as_secs_f64() * 1_000.0,
                    d.confirm_batch_calls,
                    d.confirmation_groups,
                    d.confirmation_calls,
                    d.confirmation_elapsed.as_secs_f64() * 1_000.0,
                    d.confirmed_analyses,
                );
            }
            ProfiledFomaApplyOutcome::Incomplete {
                dimension,
                value,
                limit,
                diagnostics,
            } => {
                println!(
                    "UNMEASURED word={word:?} reason=apply-budget dimension={} value={} limit={} \
                     raw_paths={} raw_bytes={} decoded_paths={} malformed_paths={} \
                     proposal_unique_candidates={} proposal_calls={} confirm_batch_calls={}",
                    dimension.label(),
                    value,
                    limit,
                    diagnostics.proposal.raw_paths,
                    diagnostics.proposal.raw_bytes,
                    diagnostics.proposal.decoded_paths,
                    diagnostics.proposal.malformed_paths,
                    diagnostics.proposal.unique_candidates,
                    diagnostics.proposal_calls,
                    diagnostics.confirm_batch_calls,
                );
            }
        }
    }
    ExitCode::SUCCESS
}
