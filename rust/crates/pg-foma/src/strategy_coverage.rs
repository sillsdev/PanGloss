//! Per-STRATEGY construct coverage: which of this crate's compilers can actually PROPOSE each
//! `CharacteristicKind`, and therefore whether that kind's `crate::capability::Disposition` is honest for the
//! compiler in use.
//!
//! # The hole this closes
//! `crate::capability::Disposition::ConfirmOnly`'s own definition (`capability.rs`) is: *"Recall-preserving only if
//! the proposer proposes the superset."* That is a claim about a PROPOSER, and until this module
//! existed nothing in the capability layer knew which proposer was in use.
//! `crate::capability::characterize` takes a bare `&Grammar`, [`crate::capability::
//! compose_envelope`] takes a `&Grammar` and a `&Plan`, and
//! `crate::enumerate::EmissionStrategy` -- the type that names WHICH compiler realizes a
//! candidate -- appeared nowhere in `capability.rs`, `coverage_ledger.rs`,
//! `conformance_coverage.rs` or `gate.rs` at all.
//!
//! The consequence was measured, not hypothesized. `Compounding` rested at a non-refusing
//! disposition on the strength of `crate::emit`'s compilers being able to propose compounds, while
//! `crate::uflexc` -- the ONLY lexicon emitter
//! `crate::enumerate::EmissionStrategy::PlanComposed` has -- emitted a structurally single-root
//! continuation graph that could not propose ANY compound (that module's own "Bounded compound
//! loop" doc). One compiler's ability was silently inherited by all three. The compound hole itself
//! is now fixed; this module fixes the ACCOUNTING that let it hide, so the next one cannot.
//!
//! The repo already has a name for this shape: the coverage-gate inheritance trap, previously seen
//! across CONSTRUCT granularity (a coarse `constructs.txt` row inheriting a fine
//! `CharacteristicKind`'s coverage). This is the same trap on a different axis -- per STRATEGY
//! rather than per construct.
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
pub const ALL_STRATEGIES: &[EmissionStrategy] = &[
    EmissionStrategy::PlanComposed,
    EmissionStrategy::TunedSurfaceProbed,
    EmissionStrategy::TemplatedUnderlyingTokens,
];

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
        EmissionStrategy::PlanComposed => plan_composed(kind),
        EmissionStrategy::TunedSurfaceProbed => tuned_surface_probed(kind),
        EmissionStrategy::TemplatedUnderlyingTokens => templated_underlying_tokens(kind),
    };
    StrategyCoverageRow {
        strategy,
        kind,
        representation,
        evidence,
    }
}

/// `EmissionStrategy::PlanComposed`'s only lexicon emitter, `crate::uflexc`, is an explicitly minimal prototype, so it is the strategy with real holes -- previously invisible to a strategy-blind account.
fn plan_composed(kind: CharacteristicKind) -> (StrategyRepresentation, &'static str) {
    use CharacteristicKind::*;
    use StrategyRepresentation::*;
    match kind {
        // uflexc emits prefix/suffix chains from `classify_affix`-classified allomorphs.
        Affixation => (
            Represents,
            "uflexc::emit_underlying_filtered -- Role::Prefix/Role::Suffix allomorphs become \
             prefix/suffix continuation-chain lines",
        ),
        // A live hole: `uflexc` skips every `MorphRuleDef::Realizational` rule wholesale, so no lexc line is ever written and the proposer returns zero candidates for any word requiring it.
        RealizationalMorphology => (
            CannotRepresent,
            "uflexc::emit_underlying_filtered -- MorphRuleDef::Realizational is reported in \
             `skipped` as `kind=realizational-rule` and `continue`d past; no lexc line is emitted \
             for the rule at all (uflexc module doc: it never attempts the syntactic \
             feature-realization mechanism RealizationalRuleDef needs)",
        ),
        // Now a real, budget-bounded, unrolled compound chain over `emit::compound_license`'s head/non-head split.
        Compounding => (
            Represents,
            "uflexc's bounded compound loop (emit::build_compound_chain + emit::compound_license) \
             -- unrolled to emit::compound_extra_levels_checked levels; before it existed this row \
             was CannotRepresent and nothing in the capability layer could say so",
        ),
        // Rule order is a property of the cascade `build_controllable` composes, not of the lexicon.
        OrderedMorphRuleApplication => (
            Represents,
            "build::build_controllable composes the Plan's Replace cascade in authored order",
        ),
        // uflexc's prefix/suffix chains self-loop, so any order of a stratum's loose rules is already a path through the emitted graph.
        UnorderedMorphRuleApplication => (
            Represents,
            "uflexc's self-looping prefix/suffix continuation chains admit every order of a \
             stratum's loose rules by construction (uflexc module doc)",
        ),
        // MPR-group state is a confirm-time mechanism in every strategy; no emitter tracks it.
        MprGroupAppend | MprGroupOverwrite => (
            Represents,
            "MPR accumulation/overwrite is enforced at confirm time (pg_rules::validity) for every \
             strategy alike; no lexicon emitter represents or filters on it",
        ),
        // Every rewrite/metathesis/epenthesis/quantifier fact belongs to the compiled rule cascade, the same one every strategy composes with its own lexicon, so these rows are strategy-invariant.
        IterativeRewrite | SimultaneousRewrite | LeftToRightRewrite | RightToLeftRewrite
        | Metathesis | Epenthesis | QuantifierPattern => (
            Represents,
            "compiled by crate::replace's rule cascade, which build::build_controllable composes \
             with the uflexc lexicon -- the same cascade every strategy uses",
        ),
        // Gating is what the Plan's Gate node is; build_controllable builds one network per partition group and unions them.
        SubruleGating => (
            Represents,
            "build::build_controllable builds one network per crate::gate partition group and \
             unions them; uflexc takes each group's own allowed_entries",
        ),
        // Not a whole-construct hole: a Prefix/Suffix-classified allomorph that also drops non-edge material is still emitted, just without the drop applied -- a documented partial, not a refusal.
        CircumfixOutputAction => (
            RepresentsWithKnownGap,
            "uflexc::emit_underlying_filtered skips every allomorph whose emit::classify_affix role \
             is not Prefix/Suffix (`role=circumfix-prefix`/`process`/`none` in `skipped`), and has \
             no structural-composite path (emit::build_structural_composites) to resynthesize \
             dropped material for the ones it does emit",
        ),
        // Reduplication is peeled outside the compiled FST for every strategy, so the lexicon emitter is not the mechanism; uflexc's own skip of these allomorphs is consistent with that division, not an extra gap.
        Reduplication => (
            Represents,
            "reduplication is handled by crate::peel outside the compiled FST for every strategy \
             (capability.rs's Reduplication arm); uflexc's `role=reduplication` skip is that same \
             division, not an extra gap",
        ),
        CoOccurrenceConstraint => (
            Represents,
            "co-occurrence adjacency is a confirm-time constraint (pg_rules::validity) for every \
             strategy; no lexicon emitter filters on it",
        ),
        NaturalClassDefinition => (
            Represents,
            "representational only -- consumed by crate::replace's pattern lowering, identically \
             for every strategy",
        ),
        MultiTable => (
            Represents,
            "per-rule owning-table threading lives in crate::replace (fix-multitable-fst-\
             compilation), shared by every strategy; uflexc encodes shapes through \
             replace::SegAlphabet",
        ),
        // Every root allomorph is emitted bare and unconditionally; the restriction is confirm's.
        StemName => (
            Represents,
            "every non-pattern root allomorph is emitted bare (uflexc module doc's \"Deliberate \
             supersets\"); stem-name restriction is discharged only by \
             pg_rules::validity::stem_name_gate_reason at confirm, identically for every strategy",
        ),
        FreeFluctuation => (
            Represents,
            "every allomorph of a multi-allomorph entry gets its own root line, uniformly -- the \
             uniform over-proposal capability.rs's FreeFluctuation arm describes",
        ),
        ProcessMorphology => (
            CannotRepresent,
            "uflexc::emit_underlying_filtered -- its own module doc lists Role::Process in the \n             SKIPPED set (\"Reduplication/Infix/CircumfixPrefix/CircumfixSuffix/Process/None is \n             skipped and reported\"); no lexc line is emitted for an in-place mutation at all",
        ),
    }
}

/// `EmissionStrategy::TunedSurfaceProbed`, the mainline whole-grammar compiler every containment witness in `crate::coverage_ledger` was written against; the other two strategies' rows are gaps against it.
fn tuned_surface_probed(kind: CharacteristicKind) -> (StrategyRepresentation, &'static str) {
    use CharacteristicKind::*;
    use StrategyRepresentation::*;
    let mainline = "emit::emit_with_budget -- the mainline whole-grammar compiler; every \
                    tests/cover_*.rs and tests/phase_c_*.rs containment witness in \
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
        ProcessMorphology => (
            Represents,
            "emit::is_structural_rule admits Role::Process unconditionally (emit.rs), routing it \n             through build_structural_composites, which replays pg_rules::morph::synthesize -- the \n             real engine -- so the mutated surface is faithful rather than spliced",
        ),
    }
}

/// `EmissionStrategy::TemplatedUnderlyingTokens` shares `emit.rs`'s morphotactic machinery but composes a real rewrite cascade; that function's own doc enumerates what it deliberately drops.
fn templated_underlying_tokens(kind: CharacteristicKind) -> (StrategyRepresentation, &'static str) {
    use CharacteristicKind::*;
    use StrategyRepresentation::*;
    let shared =
        "emit::emit_underlying_templated shares emit.rs's collect_roots/build_deriv_chain/\
                  build_slot_chain morphotactics with the mainline compiler";
    match kind {
        // `emit.rs`'s rule accessors treat Realizational exactly like AffixProcess, and this emitter uses those same accessors.
        Affixation | RealizationalMorphology => (Represents, shared),
        Compounding => (
            Represents,
            "emit::build_compound_chain -- the SAME shared unroller emit_with_budget and uflexc \
             use (emit.rs task #44 generalized it over the root-record and emitter-state types)",
        ),
        OrderedMorphRuleApplication | UnorderedMorphRuleApplication => (Represents, shared),
        MprGroupAppend | MprGroupOverwrite => (
            Represents,
            "MPR accumulation/overwrite is enforced at confirm time for every strategy; no \
             lexicon emitter represents or filters on it",
        ),
        IterativeRewrite | SimultaneousRewrite | LeftToRightRewrite | RightToLeftRewrite
        | Metathesis | Epenthesis | QuantifierPattern => (
            Represents,
            "templated_compile::compile_templated_morphotactics composes crate::replace's real \
             rewrite cascade -- the strategy's whole premise (the cascade does the phonological \
             work the surface probe would otherwise bake in)",
        ),
        SubruleGating => (
            Represents,
            "emit_underlying_templated takes crate::gate's allowed_entries with uflexc's own \
             convention (that function's own doc)",
        ),
        // Runs no composite pipeline, so a single-sided-truncation allomorph is emitted with its literal text and no drop applied -- material IS emitted, so this is a documented partial, not a refusal.
        CircumfixOutputAction => (
            RepresentsWithKnownGap,
            "emit::emit_underlying_templated's own doc: \"No composite pipeline at all\" -- \
             build_structural_composites is skipped, so a single-sided-truncation allomorph is \
             emitted with its literal InsertSegments text and no drop applied, and can miss the \
             correct underlying form",
        ),
        Reduplication => (
            Represents,
            "reduplication is peeled outside the compiled FST by crate::peel for every strategy \
             (capability.rs's Reduplication arm)",
        ),
        CoOccurrenceConstraint | NaturalClassDefinition | StemName | FreeFluctuation => (
            Represents,
            "confirm-time or representational only -- identical for every strategy; no lexicon \
             emitter filters on any of these",
        ),
        ProcessMorphology => (
            CannotRepresent,
            "emit::emit_underlying_templated's own doc: \"No composite pipeline at all\" -- with no \n             composite route there is nothing to realize an in-place mutation with",
        ),
        MultiTable => (
            Represents,
            "per-rule owning-table threading lives in crate::replace, shared by every strategy",
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
mod tests {
    use super::*;

    /// The table is total: every `(strategy, kind)` pair has a row, and every row cites something.
    #[test]
    fn every_strategy_kind_pair_has_a_cited_row() {
        for &strategy in ALL_STRATEGIES {
            for &kind in CharacteristicKind::ALL {
                let row = representation_of(strategy, kind);
                assert_eq!(row.strategy, strategy);
                assert_eq!(row.kind, kind);
                assert!(
                    !row.evidence.is_empty(),
                    "{strategy:?} x {kind:?} has an empty evidence citation"
                );
            }
        }
    }

    /// `EmissionStrategy` has no reflection, so this pins the count and each label rather than deriving it.
    #[test]
    fn all_strategies_lists_every_emission_strategy() {
        let labels: Vec<&str> = ALL_STRATEGIES.iter().map(|s| s.label()).collect();
        assert_eq!(
            labels,
            vec![
                "plan-composed",
                "tuned-surface-probed",
                "templated-underlying-tokens"
            ]
        );
    }

    /// The row this module exists for: `PlanComposed` never writes a line for a `Realizational` rule, so it cannot propose one -- exactly what a strategy-blind account cannot say.
    #[test]
    fn plan_composed_cannot_represent_realizational_morphology() {
        assert_eq!(
            representation_of(
                EmissionStrategy::PlanComposed,
                CharacteristicKind::RealizationalMorphology
            )
            .representation,
            StrategyRepresentation::CannotRepresent
        );
    }

    /// The two whole-grammar compilers CAN, which is why a strategy-blind account read the construct as covered: the union of abilities is not any one compiler's ability.
    #[test]
    fn the_whole_grammar_compilers_can_represent_realizational_morphology() {
        for &strategy in &[
            EmissionStrategy::TunedSurfaceProbed,
            EmissionStrategy::TemplatedUnderlyingTokens,
        ] {
            assert_eq!(
                representation_of(strategy, CharacteristicKind::RealizationalMorphology)
                    .representation,
                StrategyRepresentation::Represents,
                "{strategy:?}"
            );
        }
    }

    /// If the strategies never disagreed, this table would be a strategy-indexed copy of one answer.
    #[test]
    fn at_least_one_kind_is_answered_differently_by_two_strategies() {
        let disagreements: Vec<CharacteristicKind> = CharacteristicKind::ALL
            .iter()
            .copied()
            .filter(|&kind| {
                let first = representation_of(ALL_STRATEGIES[0], kind).representation;
                ALL_STRATEGIES
                    .iter()
                    .any(|&s| representation_of(s, kind).representation != first)
            })
            .collect();
        assert!(
            !disagreements.is_empty(),
            "no construct is answered differently by two strategies -- the table would be \
             strategy-blind in effect"
        );
    }

    /// Pinned so a regression in the bounded compound loop shows up here as a reviewed table edit.
    #[test]
    fn compounding_is_representable_by_every_strategy_now() {
        for &strategy in ALL_STRATEGIES {
            assert_eq!(
                representation_of(strategy, CharacteristicKind::Compounding).representation,
                StrategyRepresentation::Represents,
                "{strategy:?}"
            );
        }
    }

    // The premise `crate::capability::StrategyEnvelope::global` rests on: one compiler with no gap.
    #[test]
    fn some_strategy_represents_every_kind() {
        assert_eq!(
            strategies_representing_every_kind(),
            vec![EmissionStrategy::TunedSurfaceProbed],
            "the mainline compiler is the one with no recorded gap; if that changes, \
             capability::StrategyEnvelope::global's identity to the compiler-blind verdict changes \
             with it"
        );
    }

    #[test]
    fn unrepresentable_kinds_names_every_hole_and_leaves_the_mainline_clear() {
        assert_eq!(
            unrepresentable_kinds(EmissionStrategy::PlanComposed),
            vec![
                CharacteristicKind::RealizationalMorphology,
                // uflexc lists Role::Process in its own skipped set; no lexc line is emitted at all.
                CharacteristicKind::ProcessMorphology
            ]
        );
        assert_eq!(
            unrepresentable_kinds(EmissionStrategy::TemplatedUnderlyingTokens),
            vec![CharacteristicKind::ProcessMorphology],
            "the templated emitter's own doc says it has no composite pipeline, so it cannot \
             realize an in-place mutation"
        );
        assert!(unrepresentable_kinds(EmissionStrategy::TunedSurfaceProbed).is_empty());
    }

    #[test]
    fn strategies_that_represent_excludes_only_the_holed_compiler() {
        assert_eq!(
            strategies_that_represent(CharacteristicKind::RealizationalMorphology),
            vec![
                EmissionStrategy::TunedSurfaceProbed,
                EmissionStrategy::TemplatedUnderlyingTokens
            ]
        );
        assert_eq!(
            strategies_that_represent(CharacteristicKind::Affixation).len(),
            ALL_STRATEGIES.len()
        );
    }
}
