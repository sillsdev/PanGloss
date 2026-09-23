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

fn tier_requires_unproven_build(tier: &FomaTier) -> bool {
    matches!(tier, FomaTier::Partial { .. })
}

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

fn compile_proposer_with_profile_policy(
    g: &Grammar,
    allow_incomplete: bool,
) -> (Result<FomaProposer>, CompileProfile) {
    let mut profile = CompileProfileBuilder::production();
    if !allow_incomplete {
        if let Err(diagnostics) =
            crate::capability_gate::refuse_unless_admitted(g, FomaProposer::EMISSION_STRATEGY)
        {
            return (
                Err(FomaError::CapabilityRefused(diagnostics)),
                profile.finish(None, None),
            );
        }
    }
    let result = emit::emit_with_budget_profiled(
        g,
        crate::precision::PrecisionConfig::Strip,
        Some(&mut profile),
    );
    finish_profiled_compile(result, profile, allow_incomplete)
}

fn finish_profiled_compile(
    result: crate::emit::EmitResult,
    mut profile: CompileProfileBuilder,
    allow_incomplete: bool,
) -> (Result<FomaProposer>, CompileProfile) {
    if matches!(result.report.tier, FomaTier::Unsupported { .. }) {
        return (
            Err(FomaError::Unsupported(Box::new(result.report))),
            profile.finish(None, None),
        );
    }
    if tier_requires_unproven_build(&result.report.tier) && !allow_incomplete {
        return (
            Err(FomaError::Incomplete(Box::new(result.report))),
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
