pub use pg_foma_runtime::analyzer::{
    prepare_network_for_apply, read_foma_binary_payload, FomaError, FomaProposer, ProposalCounts,
    ProposalDiagnostics, Result,
};

use std::time::Instant;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use pg_grammar::model::Grammar;

use crate::emit::{self, FomaTier};
use crate::profile::{CompileProfile, CompileProfileBuilder, CompileStage};

pub fn compile_proposer(g: &Grammar) -> Result<FomaProposer> {
    compile_proposer_with_profile(g).0
}

pub fn new_proposer(g: &Grammar) -> Result<FomaProposer> {
    compile_proposer(g)
}

pub fn compile_proposer_with_profile(g: &Grammar) -> (Result<FomaProposer>, CompileProfile) {
    compile_proposer_with_profile_policy(g, false)
}

pub fn new_proposer_with_profile(g: &Grammar) -> (Result<FomaProposer>, CompileProfile) {
    compile_proposer_with_profile(g)
}

#[cfg(feature = "developer-tools")]
pub fn compile_proposer_unproven_with_profile(
    g: &Grammar,
) -> (Result<FomaProposer>, CompileProfile) {
    compile_proposer_with_profile_policy(g, true)
}

pub fn compile_proposer_with_config(
    g: &Grammar,
    config: &crate::construction::FstCompileConfig,
) -> (Result<FomaProposer>, CompileProfile) {
    compile_configured(g, config, true)
}

/// Complete construction without the advisory capability precheck, for differential measurement.
/// The emitter's representability and containment refusals still apply; partial output is rejected.
#[doc(hidden)]
pub fn compile_proposer_for_measurement_with_config(
    g: &Grammar,
    config: &crate::construction::FstCompileConfig,
) -> (Result<FomaProposer>, CompileProfile) {
    compile_configured(g, config, false)
}

fn compile_proposer_with_profile_policy(
    g: &Grammar,
    bypass_capability_gate: bool,
) -> (Result<FomaProposer>, CompileProfile) {
    compile_configured(
        g,
        &crate::construction::FstCompileConfig::default(),
        !bypass_capability_gate,
    )
}

fn compile_configured(
    g: &Grammar,
    config: &crate::construction::FstCompileConfig,
    check_capability: bool,
) -> (Result<FomaProposer>, CompileProfile) {
    use crate::construction::{
        ConstructionAttempt, ConstructionReport, ConstructionStrategy, StrategyRefusal,
        StrategySelection,
    };
    let started = Instant::now();
    let mut config = config.clone();
    let mut report = ConstructionReport {
        requested_config: Some(config.clone()),
        config: Some(config.clone()),
        ..Default::default()
    };
    let mut structural_limits = crate::construction::StructuralClosureLimits::default();
    let strategies = config.strategies().and_then(|strategies| {
        structural_limits = crate::construction::StructuralClosureLimits::from_overrides(
            config.bounds.structural_work,
            config.bounds.structural_bytes,
        )?;
        config.bounds.structural_work = Some(structural_limits.work_limit);
        config.bounds.structural_bytes = Some(structural_limits.retained_bytes_limit);
        if crate::compose_budget::clamp_chain_depth_cap(config.bounds.compound_depth)
            != config.bounds.compound_depth
        {
            return Err(
                "configured compound depth exceeds the absolute chain-depth ceiling".to_string(),
            );
        }
        let depth = config.bounds.derivation_depth.map_or_else(
            || {
                emit::parse_deriv_chain_budget(
                    std::env::var("HC_DERIV_CHAIN_DEPTH_BUDGET").ok().as_deref(),
                )
            },
            Ok,
        )?;
        config.bounds.derivation_depth = Some(depth);
        Ok(strategies)
    });
    report.config = Some(config.clone());
    let strategies = match strategies {
        Ok(strategies) => strategies,
        Err(error) => {
            report.configuration_error = Some(error);
            let mut profile = CompileProfileBuilder::production().finish(None, None);
            profile.construction = report.clone();
            return (
                Err(FomaError::ConstructionRefused(Box::new(report))),
                profile,
            );
        }
    };
    let mut last = None;
    for strategy in strategies {
        let attempt_start = Instant::now();
        let mut profile = CompileProfileBuilder::production();
        let (result, mut finished) = if strategy == ConstructionStrategy::Factored {
            (
                Err(FomaError::StrategyNotImplemented(strategy)),
                profile.finish(None, None),
            )
        } else if check_capability {
            match crate::capability_gate::refuse_unless_admitted(g, FomaProposer::EMISSION_STRATEGY)
            {
                Err(diagnostics) => (
                    Err(FomaError::CapabilityRefused(diagnostics)),
                    profile.finish(None, None),
                ),
                Ok(()) => {
                    let emitted = emit::emit_configured_surface(
                        g,
                        &config.bounds,
                        structural_limits,
                        &mut profile,
                    );
                    finish_profiled_compile(emitted, profile)
                }
            }
        } else {
            let emitted =
                emit::emit_configured_surface(g, &config.bounds, structural_limits, &mut profile);
            finish_profiled_compile(emitted, profile)
        };
        let emitted_report = match &result {
            Ok(proposer) => proposer.report.as_ref(),
            Err(FomaError::Unsupported(report) | FomaError::LexcCompileFailed(report)) => {
                Some(report.as_ref())
            }
            _ => None,
        };
        let refusal = match &result {
            Ok(_) => None,
            Err(FomaError::StrategyNotImplemented(_)) => Some(StrategyRefusal::NotImplemented),
            Err(FomaError::ConstructionRefused(_)) => Some(StrategyRefusal::InvalidConfiguration),
            Err(FomaError::CapabilityRefused(_)) => Some(StrategyRefusal::Capability),
            Err(FomaError::LexcCompileFailed(_)) => Some(StrategyRefusal::LexcCompileFailed),
            Err(_)
                if emitted_report.is_some_and(|report| {
                    report.enum_budget_exceeded.is_some()
                        || report.closure_evidence.as_ref().is_some_and(|closure| {
                            matches!(
                                closure.terminal,
                                crate::characterization::ClosureTerminal::Incomplete(
                                    crate::characterization::ClosureStopReason::WorkBudgetReached
                                    | crate::characterization::ClosureStopReason::DepthBudgetReached
                                    | crate::characterization::ClosureStopReason::ResourceBudgetReached
                                ) | crate::characterization::ClosureTerminal::Refused(
                                    crate::characterization::ClosureStopReason::WorkBudgetReached
                                    | crate::characterization::ClosureStopReason::DepthBudgetReached
                                    | crate::characterization::ClosureStopReason::ResourceBudgetReached
                                )
                            )
                        })
                }) =>
            {
                Some(StrategyRefusal::LimitExceeded)
            }
            Err(_) => Some(StrategyRefusal::Incomplete),
        };
        report.attempts.push(ConstructionAttempt {
            strategy,
            elapsed_millis: attempt_start.elapsed().as_millis() as u64,
            process_peak_rss_kib: crate::construction::process_peak_rss_kib(),
            states: finished.final_state_count,
            arcs: finished.final_arc_count,
            refusal,
            detail: if strategy == ConstructionStrategy::Factored {
                Some("factored construction is not implemented".to_string())
            } else {
                result.as_ref().err().map(ToString::to_string)
            },
            closure: emitted_report.and_then(|report| report.closure_evidence.clone()),
            bound_refusal: emitted_report
                .and_then(|report| report.enum_budget_exceeded.as_ref())
                .map(|bound| crate::construction::ConstructionLimit {
                    measure: bound.measure.to_string(),
                    value: bound.value,
                    limit: bound.limit,
                }),
            compile_phase: finished
                .stages
                .last()
                .map_or(
                    if strategy == ConstructionStrategy::Factored {
                        "dispatch"
                    } else {
                        "admission"
                    },
                    |stage| stage.stage.label(),
                )
                .to_string(),
        });
        if result.is_ok() {
            report.selected = Some(strategy);
        }
        finished.total_elapsed_millis = started.elapsed().as_millis() as u64;
        finished.construction = report.clone();
        let complete = result.is_ok();
        last = Some((result, finished));
        if complete {
            break;
        }
    }
    let (result, profile) = last.expect("validated construction order is nonempty");
    if result.is_err() && config.strategy == StrategySelection::Auto {
        (
            Err(FomaError::ConstructionRefused(Box::new(report))),
            profile,
        )
    } else {
        (result, profile)
    }
}

fn finish_profiled_compile(
    result: crate::emit::EmitResult,
    mut profile: CompileProfileBuilder,
) -> (Result<FomaProposer>, CompileProfile) {
    if matches!(
        result.report.tier,
        FomaTier::Unsupported { .. } | FomaTier::CannotRepresent { .. }
    ) {
        return (
            Err(FomaError::Unsupported(Box::new(result.report))),
            profile.finish(None, None),
        );
    }
    let opts = FomaOptions::default();
    let lexc_parse_start = Instant::now();
    let parsed = fsm_lexc_parse_string(&opts, None, &result.lexc_source);
    profile.push_stage(CompileStage::LexcParse, lexc_parse_start.elapsed());
    match parsed {
        Some(mut net) => {
            prepare_network_for_apply(&mut net);
            let final_state_count = net.statecount;
            let final_arc_count = net.arccount;
            let proposer = FomaProposer::from_precompiled_network(&net, result.report);
            (
                Ok(proposer),
                profile.finish(Some(final_state_count), Some(final_arc_count)),
            )
        }
        None => (
            Err(FomaError::LexcCompileFailed(Box::new(result.report))),
            profile.finish(None, None),
        ),
    }
}

#[cfg(feature = "developer-tools")]
pub fn compile_proposer_unproven(g: &Grammar) -> Result<FomaProposer> {
    compile_proposer_unproven_with_profile(g).0
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    include!("analyzer_compile_tests.rs");
}
