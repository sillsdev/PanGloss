//! The capability envelope consulted BEFORE a compiler runs, rather than inferred from its failure.
//!
//! ADR-0001 decides that a grammar a backend cannot represent faithfully is hard-failed at compile
//! time with a typed diagnostic naming the construct. The envelope that computes that verdict has
//! existed for some time, but nothing on a compile path consulted it: correctness was decided by
//! whatever the emitter happened to report afterwards, which arrives as a compile artifact rather
//! than as a capability refusal a selector can read.
//!
//! Artifact production and representability are separate axes (`CONTEXT.md`, "Three axes that must
//! never merge"); this gate answers only representability.

use pg_grammar::model::Grammar;

use crate::backend_selection::{select_backends, BackendSelection};
use crate::capability::{CapabilityDiagnostic, CompileDecision};
use crate::enumerate::EmissionStrategy;
use crate::grammar_semantics::GrammarSemantics;

/// Characterizes `g` once and answers whether `strategy` may compile it, without compiling.
///
/// The whole point is that this runs before any emission: emitting is real work (a large lexicon
/// reaches hundreds of thousands of lexc lines), and a construct the backend cannot represent is
/// knowable from the characterization alone.
///
/// A convenience wrapper: derives a `BackendSelection` and delegates to
/// `refuse_unless_admitted_in`. A caller that already holds a `BackendSelection` for this compile
/// attempt should call that directly instead of paying for a second grammar walk.
pub fn refuse_unless_admitted(
    g: &Grammar,
    strategy: EmissionStrategy,
) -> Result<(), Vec<CapabilityDiagnostic>> {
    let semantics = GrammarSemantics::derive(g);
    refuse_unless_admitted_in(&select_backends(&semantics), strategy)
}

/// `refuse_unless_admitted` over an already-computed `BackendSelection` -- the one owner of "is
/// this backend admitted" (`BackendSelection::decision_for`), never re-derived here. Fails CLOSED
/// on a missing report, matching `decision_for`'s own contract; `admission_single_owner_gate.rs`
/// measures that this arm is a guard, not live policy, for every discovered conformance fixture.
pub fn refuse_unless_admitted_in(
    selection: &BackendSelection,
    strategy: EmissionStrategy,
) -> Result<(), Vec<CapabilityDiagnostic>> {
    match selection.decision_for(strategy) {
        CompileDecision::Admit | CompileDecision::ConfirmOnly => Ok(()),
        CompileDecision::Refuse(diagnostics) => Err(diagnostics),
    }
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
