//! Per-STRATEGY construct coverage: which of this crate's compilers can actually PROPOSE each
//! `CharacteristicKind`, and therefore whether that kind's `crate::capability::Disposition` is honest for the
//! compiler in use.
//!
//! # The hole this closes
//! `crate::capability::Disposition::ConfirmOnly`'s own definition (`capability.rs`) is: *"Recall-preserving only if
//! the proposer proposes the superset."* That is a claim about a PROPOSER, so the capability layer has to know
//! which proposer is in use. This table records, per `EmissionStrategy`, which constructs that
//! compiler can represent, so one compiler's ability is never silently inherited by another (the
//! coverage-gate inheritance trap, seen per STRATEGY rather than per construct). Only
//! `TunedSurfaceProbed` exists today; a comparison backend would need its own rows here.
//!
//! # What this module is, and is not
//! It is a hand-curated, source-cited table: for each `(EmissionStrategy, CharacteristicKind)`
//! pair, one `StrategyRepresentation` plus the citation that establishes it. It is deliberately
//! curated rather than derived, for the same reason `crate::coverage_ledger` is: a one-time
//! REVIEWED table over the current model, deliberately without source-AST/reflection
//! infrastructure. Nothing here inspects an
//! emitter at runtime; a reviewer reads the emitter and writes the row.
//!
//! It is NOT a second disposition table. `CharacteristicKind::default_disposition` remains the
//! single source of truth for what a construct costs IN GENERAL; this table says only whether a
//! particular compiler's proposer can represent it at all. The two are combined -- never merged --
//! by `crate::capability::compose_envelope_for_strategy`.
//!
//! # Why this is a separate table and NOT a strategy-keyed `characterize` memo
//! `crate::grammar_semantics::GrammarSemantics` memoizes `characterize()` keyed on the grammar
//! alone. That memo is CORRECT and stays: `characterize` answers "which constructs does this
//! grammar CONTAIN", which is a property of the grammar and cannot vary by compiler. What varies
//! by compiler is "can this proposer represent that construct" -- a property of the COMPILER, with
//! no grammar input at all. Re-keying the memo on `(grammar, strategy)` would force a full
//! `characterize` walk (real `foma::types::Fsm` construction for every `Simultaneous`-mode subrule)
//! once per strategy to recompute facts that are provably identical, re-introducing exactly the
//! per-candidate cost this crate already removed -- and it
//! would put a compiler fact inside a type whose whole contract is "pure function of `&Grammar`".
//! So the strategy-dependent half is SPLIT OUT here instead.
//! `tests/strategy_aware_capability_gate.rs`'s
//! `two_strategies_get_their_own_answers_from_one_shared_semantics` pins that the split actually
//! delivers per-strategy answers through a single shared `GrammarSemantics` -- in either asking
//! order, so a memo poisoned by whoever asked first cannot pass it.

use crate::capability::CharacteristicKind;
use crate::enumerate::EmissionStrategy;

/// Whether one `EmissionStrategy`'s proposer can represent one `CharacteristicKind`.
///
/// Three-valued on purpose. A two-valued table would have to file every *documented partial* gap
/// as either a clean "represents" (dishonest -- the gap is written down in the emitter's own doc)
/// or a hard "cannot represent" (over-claiming -- the emitter demonstrably emits material for the
/// construct, and refusing on that basis would remove candidates that today compile correctly for
/// the overwhelming majority of instances). Neither is the truth, so the truth gets its own
/// variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrategyRepresentation {
    /// This strategy's proposer emits material covering the construct, with no gap this crate's
    /// own sources record. The `ConfirmOnly` precondition ("the proposer proposes the superset")
    /// holds for this compiler.
    Represents,
    /// The proposer emits material for the construct, but a specific, cited case exists where it
    /// can under-propose. Folded in as `crate::capability::CompileDecision::ConfirmOnly`: never
    /// better than confirm-gated, never a hard refusal (the construct IS proposed, just not
    /// provably as a superset).
    RepresentsWithKnownGap,
    /// The proposer emits NOTHING for the construct: a whole-construct recall hole. The
    /// `ConfirmOnly` precondition is FALSE for this compiler, so no disposition short of a refusal
    /// is honest -- confirm cannot prune a candidate set into existence.
    CannotRepresent,
}

impl StrategyRepresentation {
    /// Stable identifier for reports and serialized artifacts.
    pub fn label(self) -> &'static str {
        match self {
            Self::Represents => "represents",
            Self::RepresentsWithKnownGap => "represents-with-known-gap",
            Self::CannotRepresent => "cannot-represent",
        }
    }
}

/// One curated row of the per-strategy account: the verdict plus the source citation that
/// establishes it. Every `CannotRepresent`/`RepresentsWithKnownGap` row cites the emitter code (or
/// its own doc) that a reviewer can check; no row is safe by assumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrategyCoverageRow {
    pub strategy: EmissionStrategy,
    pub kind: CharacteristicKind,
    pub representation: StrategyRepresentation,
    /// Where in this crate the verdict is established (module/function, not a line number -- line
    /// numbers rot).
    pub evidence: &'static str,
}

/// Every `EmissionStrategy` variant -- hand-maintained, exactly like
/// `CharacteristicKind::ALL` (Rust has no enum reflection). Adding a variant and forgetting this
/// constant is caught by `representation_of`'s exhaustive, catch-all-free `match` on `strategy`
/// only if the new variant is also given arms there, which it must be for the build to pass.
pub const ALL_STRATEGIES: &[EmissionStrategy] = &[EmissionStrategy::TunedSurfaceProbed];

/// The curated table (module doc). Exhaustively matched on BOTH axes with no catch-all arm, the
/// same discipline `crate::capability::characterize` and
/// `crate::coverage_ledger::containment_evidence_for` hold themselves to: adding an
/// `EmissionStrategy` variant or a `CharacteristicKind` variant breaks this build until a
/// reviewer gives the new pair an explicit verdict. That compile break IS the mechanism -- a new
/// compiler cannot silently inherit the incumbent's coverage.
pub fn representation_of(
    strategy: EmissionStrategy,
    kind: CharacteristicKind,
) -> StrategyCoverageRow {
    let (representation, evidence) = match strategy {
        EmissionStrategy::TunedSurfaceProbed => tuned_surface_probed(kind),
    };
    StrategyCoverageRow {
        strategy,
        kind,
        representation,
        evidence,
    }
}

/// `EmissionStrategy::TunedSurfaceProbed`, the mainline whole-grammar compiler every containment witness in `crate::coverage_ledger` is written against.
fn tuned_surface_probed(kind: CharacteristicKind) -> (StrategyRepresentation, &'static str) {
    use CharacteristicKind::*;
    use StrategyRepresentation::*;
    let mainline = "emit::emit_with_budget -- the mainline whole-grammar compiler; every \
                    tests/cover_*.rs containment witness in \
                    coverage_ledger::containment_evidence_for exercises this compiler";
    match kind {
        Affixation
        | RealizationalMorphology
        | Compounding
        | OrderedMorphRuleApplication
        | UnorderedMorphRuleApplication
        | MprGroupAppend
        | MprGroupOverwrite
        | IterativeRewrite
        | SimultaneousRewrite
        | LeftToRightRewrite
        | RightToLeftRewrite
        | Metathesis
        | Epenthesis
        | SubruleGating
        | CircumfixOutputAction
        | Reduplication
        | CoOccurrenceConstraint
        | NaturalClassDefinition
        | MultiTable
        | QuantifierPattern
        | StemName
        | FreeFluctuation => (Represents, mainline),
        CrossTableRespelling => (
            Represents,
            "emit::collect_roots unions emit::cross_table_root_surface's final-table spelling of \
             every inner-stratum root into the root's lexc variants, rule or no rule; \
             tests/cross_table_root_respelling_gate.rs holds it oracle-exact",
        ),
        ProcessMorphology => (
            Represents,
            "emit::is_structural_rule admits Role::Process unconditionally (emit.rs), routing it \n             through build_structural_composites, which replays pg_rules::morph::synthesize -- the \n             real engine -- so the mutated surface is faithful rather than spliced",
        ),
    }
}

/// Every `CharacteristicKind` `strategy`'s proposer emits nothing at all for -- the whole-
/// construct recall holes. Empty for a compiler with no such hole.
pub fn unrepresentable_kinds(strategy: EmissionStrategy) -> Vec<CharacteristicKind> {
    CharacteristicKind::ALL
        .iter()
        .copied()
        .filter(|&kind| {
            representation_of(strategy, kind).representation
                == StrategyRepresentation::CannotRepresent
        })
        .collect()
}

/// Every `EmissionStrategy` whose row is `StrategyRepresentation::Represents` for EVERY
/// `CharacteristicKind` -- a compiler this account records no gap of any size against.
///
/// This is a structural premise of `crate::capability::StrategyEnvelope::global`, not a report. That
/// function derives the whole-grammar verdict as the best any compiler offers, and it equals the
/// verdict a single compiler-blind pass would produce exactly when some compiler contributes a
/// coverage floor of `Admit` for every construct -- i.e. when this list is non-empty. Empty it, and
/// the whole-grammar verdict starts depending on which constructs a grammar happens to use.
pub fn strategies_representing_every_kind() -> Vec<EmissionStrategy> {
    ALL_STRATEGIES
        .iter()
        .copied()
        .filter(|&strategy| {
            CharacteristicKind::ALL.iter().all(|&kind| {
                representation_of(strategy, kind).representation
                    == StrategyRepresentation::Represents
            })
        })
        .collect()
}

/// Every `EmissionStrategy` whose proposer emits at least SOME material for `kind` (i.e. is not
/// `StrategyRepresentation::CannotRepresent`). This is the set a coverage claim for `kind` has to
/// be measured against: evidence demonstrated on a strict subset of it is INHERITED coverage, which
/// is the failure this module exists to make visible -- see
/// `crate::coverage_ledger::LedgerRow::strategies_unwitnessed`.
pub fn strategies_that_represent(kind: CharacteristicKind) -> Vec<EmissionStrategy> {
    ALL_STRATEGIES
        .iter()
        .copied()
        .filter(|&strategy| {
            representation_of(strategy, kind).representation
                != StrategyRepresentation::CannotRepresent
        })
        .collect()
}

#[cfg(test)]
mod tests;
