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

/// `EmissionStrategy::PlanComposed`, whose ONLY lexicon emitter is `crate::uflexc`
/// (`crate::build::build_controllable` is the interpreter; that module's own doc names uflexc as
/// its emitter). uflexc is an explicitly minimal prototype -- "deliberately simpler (self-looping
/// prefix/suffix chains rather than `emit.rs`'s rule-count-bounded, template-aware derivation
/// layers)", its own module doc -- so it is the strategy with real holes, and the one whose holes
/// were previously invisible.
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
        // THE LIVE HOLE OF EXACTLY THE COMPOUNDING SHAPE. `uflexc`'s mrule loop matches
        // `MorphRuleDef::Realizational` and does `skipped.push(".. kind=realizational-rule");
        // continue;` -- the whole rule, never one allomorph -- with its own comment stating the
        // reason: "this module never attempts the syntactic feature-realization mechanism
        // `RealizationalRuleDef` needs at all". No lexc line is ever written for such a rule, so
        // the compiled net has no arc carrying its morph tag and the proposer returns ZERO
        // candidates for any word requiring it. `RealizationalMorphology` rests at
        // `Disposition::ConfirmOnly`, whose precondition is that the proposer proposes the
        // superset; for this compiler that precondition is simply false.
        RealizationalMorphology => (
            CannotRepresent,
            "uflexc::emit_underlying_filtered -- MorphRuleDef::Realizational is reported in \
             `skipped` as `kind=realizational-rule` and `continue`d past; no lexc line is emitted \
             for the rule at all (uflexc module doc: it never attempts the syntactic \
             feature-realization mechanism RealizationalRuleDef needs)",
        ),
        // Was CannotRepresent until the bounded compound loop landed; now a real, budget-bounded,
        // unrolled compound chain over `emit::compound_license`'s head/non-head split.
        Compounding => (
            Represents,
            "uflexc's bounded compound loop (emit::build_compound_chain + emit::compound_license) \
             -- unrolled to emit::compound_extra_levels_checked levels; before it existed this row \
             was CannotRepresent and nothing in the capability layer could say so",
        ),
        // Rule ORDER is a property of the cascade `build_controllable` composes, not of the
        // lexicon; uflexc's chains are order-agnostic and the cascade preserves authored order.
        OrderedMorphRuleApplication => (
            Represents,
            "build::build_controllable composes the Plan's Replace cascade in authored order",
        ),
        // uflexc's prefix/suffix chains SELF-LOOP (its own module doc), so any order of the loose
        // rules in a stratum is already a path through the emitted graph -- the ordering-union
        // superset `UnorderedOrderingUnionPredicate` describes, reached structurally.
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
        // Every rewrite-mode/direction/metathesis/epenthesis/quantifier fact belongs to the
        // compiled rule cascade (crate::replace), which `build_controllable` composes with the
        // uflexc lexicon -- the SAME cascade the other two strategies use. These rows are
        // therefore strategy-invariant.
        IterativeRewrite | SimultaneousRewrite | LeftToRightRewrite | RightToLeftRewrite
        | Metathesis | Epenthesis | QuantifierPattern => (
            Represents,
            "compiled by crate::replace's rule cascade, which build::build_controllable composes \
             with the uflexc lexicon -- the same cascade every strategy uses",
        ),
        // Gating is what the Plan's Gate node IS; build_controllable builds one network per
        // partition group and unions them.
        SubruleGating => (
            Represents,
            "build::build_controllable builds one network per crate::gate partition group and \
             unions them; uflexc takes each group's own allowed_entries",
        ),
        // A drops-LHS-material allomorph classifies `CircumfixPrefix`/`Process`/`None` under
        // `emit::classify_affix` whenever the dropped part is not at an edge, and uflexc's mrule
        // loop `skipped.push("... role={other}")`s every role that is not exactly Prefix/Suffix.
        // But it is NOT a whole-construct hole: an allomorph that drops a trailing LHS part while
        // inserting only leading material still classifies `Role::Prefix` and IS emitted -- with
        // the dropped part not actually dropped, i.e. emitted but not faithful. Both halves are
        // real, so this is the documented-partial verdict, not a refusal.
        CircumfixOutputAction => (
            RepresentsWithKnownGap,
            "uflexc::emit_underlying_filtered skips every allomorph whose emit::classify_affix role \
             is not Prefix/Suffix (`role=circumfix-prefix`/`process`/`none` in `skipped`), and has \
             no structural-composite path (emit::build_structural_composites) to resynthesize \
             dropped material for the ones it does emit",
        ),
        // Reduplication is peeled OUTSIDE the compiled FST for every strategy (crate::peel;
        // capability.rs's Reduplication arm: "peeled, never compiled into the FST itself"), so the
        // lexicon emitter is not the mechanism. uflexc additionally skips `role=reduplication`
        // allomorphs, which is consistent with -- not additional to -- that division.
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
    }
}

/// `EmissionStrategy::TunedSurfaceProbed` -- `emit::emit_with_budget` plus the junction probe,
/// pre-expansion and structural-composite pipelines, reached via `analyzer::FomaProposer::new`.
/// This is the mainline, whole-grammar compiler every `tests/cover_*.rs` containment witness in
/// `crate::coverage_ledger` was written against, and it is the reference against which the other
/// two strategies' rows are gaps.
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
    }
}

/// `EmissionStrategy::TemplatedUnderlyingTokens` -- `emit::emit_underlying_templated` plus a real
/// compiled rewrite cascade, via `templated_compile::compile_templated_morphotactics`. Shares
/// `emit.rs`'s morphotactic machinery (so it handles `Realizational` exactly as the mainline does),
/// but that function's own doc enumerates what it deliberately drops.
fn templated_underlying_tokens(kind: CharacteristicKind) -> (StrategyRepresentation, &'static str) {
    use CharacteristicKind::*;
    use StrategyRepresentation::*;
    let shared =
        "emit::emit_underlying_templated shares emit.rs's collect_roots/build_deriv_chain/\
                  build_slot_chain morphotactics with the mainline compiler";
    match kind {
        // `emit.rs`'s rule accessors treat Realizational exactly like AffixProcess (morpheme,
        // allomorphs, required_syn_fs), and this emitter uses those same accessors.
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
        // The emitter's own doc, verbatim: it runs "No composite pipeline at all"
        // (`preexpand::build_composites_with_mode`, `emit::build_structural_composites`), which
        // under `TextMode::SurfaceProbed` is exactly what gives a dropped-material allomorph its
        // correct output. The ordinary two-entry path emits the literal InsertSegments text with
        // no drop applied, so it "can genuinely MISS the correct underlying form for a root that
        // needs the drop, if no OTHER allomorph of the same rule happens to cover it
        // unconditionally". Material IS emitted, so this is a documented partial, not a refusal.
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

    /// `ALL_STRATEGIES` really is every variant. `EmissionStrategy` has no reflection, so this
    /// pins the count and each label rather than deriving it.
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

    /// THE ROW THIS MODULE EXISTS FOR. `PlanComposed`'s only lexicon emitter never writes a line
    /// for a `MorphRuleDef::Realizational` rule, so it cannot propose one -- exactly the shape the
    /// compounding hole had, and exactly what a strategy-blind account cannot say.
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

    /// ...and the two whole-grammar compilers CAN, which is precisely why a strategy-blind account
    /// read the construct as covered. The union of the three compilers' abilities is not any one
    /// compiler's ability.
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

    /// The strategies genuinely disagree -- if they did not, this table would be a
    /// strategy-indexed copy of one answer and the whole module would be theatre.
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

    /// The compounding row is now `Represents` for every strategy (the hole is fixed), which is
    /// why the accounting -- not the hole -- is what this change is about. Pinned so a regression
    /// in the bounded compound loop shows up here as a reviewed table edit.
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

    #[test]
    fn unrepresentable_kinds_reports_the_plan_composed_hole_and_nothing_for_the_mainline() {
        assert_eq!(
            unrepresentable_kinds(EmissionStrategy::PlanComposed),
            vec![CharacteristicKind::RealizationalMorphology]
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
