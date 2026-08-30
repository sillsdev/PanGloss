//! The capability envelope consulted BEFORE a compiler runs, rather than inferred from its failure.
//!
//! ADR-0001 decides that a grammar a backend cannot represent faithfully is hard-failed at compile
//! time with a typed diagnostic naming the construct. The envelope that computes that verdict has
//! existed for some time, but nothing on a compile path consulted it: correctness was decided by
//! whatever the emitter happened to report afterwards, which arrives as a compile artifact rather
//! than as a capability refusal a selector can read.
//!
//! # Why this is safe for the surface probe and the templated route
//! Measured by `tests/envelope_agrees_with_compiler_gate.rs` over every discovered fixture x every
//! backend: 183 observations, of which 3 are backends the envelope refuses while their build
//! nonetheless succeeds -- all three `TemplatedUnderlyingTokens`, none `TunedSurfaceProbed` -- so
//! making the envelope authoritative for the surface probe (`analyzer::FomaProposer::new`) and the
//! templated route (`completed_build::compile_completed_backend`) cannot lose a capability either
//! tree has. The inventory is named row-by-row in
//! `docs/research/envelope-inventory-snapshot.md`.
//!
//! `PlanComposed` is the different case, and this gate deliberately does not refuse it: artifact
//! production and representability are separate axes (`CONTEXT.md`, "Three axes that must never
//! merge"). Pinned by `plan_composed_has_no_production_compile_arm`.
//! No production caller could ever select `PlanComposed` regardless of what this gate says, unlike
//! the templated route before this file wired it in. `crate::build::unbuildable_markers`'s own doc
//! independently records a marker-bearing `PlanComposed` plan building a network that "proposed
//! nothing for 19 of 20 corpus words": the build succeeds and the result under-generates, so a
//! `PlanComposed` refusal is right even judged by build success alone, before this check's
//! unconditional reason is counted at all. The remaining 3 `TemplatedUnderlyingTokens` rows are
//! real, open compiler defects.

use pg_grammar::model::Grammar;

use crate::backend_selection::select_backends;
use crate::capability::CapabilityDiagnostic;
use crate::enumerate::EmissionStrategy;
use crate::grammar_semantics::GrammarSemantics;

/// Characterizes `g` once and answers whether `strategy` may compile it, without compiling.
///
/// The whole point is that this runs before any emission: emitting is real work (a large lexicon
/// reaches hundreds of thousands of lexc lines), and a construct the backend cannot represent is
/// knowable from the characterization alone.
///
/// A backend with no report at all is admitted rather than refused. "I could not look" must never
/// read as a refusal any more than it may read as a pass; a strategy absent from the envelope is
/// outside this gate's knowledge, and inventing a verdict for it would be the silent guess this
/// module exists to remove.
pub fn refuse_unless_admitted(
    g: &Grammar,
    strategy: EmissionStrategy,
) -> Result<(), Vec<CapabilityDiagnostic>> {
    let semantics = GrammarSemantics::derive(g);
    let selection = select_backends(&semantics);
    let Some(report) = selection.report_for(strategy) else {
        return Ok(());
    };
    if report.can_represent() {
        return Ok(());
    }
    Err(report.declined_on().to_vec())
}

/// One line per refused construct, in the vocabulary ADR-0001 asks a refusal to carry.
pub fn render_refusal(diagnostics: &[CapabilityDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|d| {
            format!(
                "predicate={} construct={} witness={}",
                d.predicate, d.construct, d.witness
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}
