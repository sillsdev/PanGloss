//! Per-STRATEGY construct coverage COLLECTED BY COMPILING, not asserted: a witness here means one
//! named backend really compiled a real grammar that really contains the construct.
//!
//! # Why a second coverage module
//! `crate::strategy_coverage` answers the same question from a hand-curated table, and
//! `crate::coverage_ledger` derives its `LedgerRow::strategies_unwitnessed` from hand-written
//! citations naming which strategies a merged test file was demonstrated on. Both are reviewed
//! prose: a citation can name a strategy nobody ever ran, and an unwitnessed count derived from
//! citations measures how the citations were written rather than what the compilers can do. This
//! module removes the reviewer from the loop for the POSITIVE half — `observe_grammar` calls
//! `crate::capability::characterize` for the constructs a grammar contains, then invokes each
//! backend's real compile entry point, and credits a `(kind, backend)` pair only when that
//! backend's own compile returned `Ok`.
//!
//! # A run yields positive evidence only
//! Compiling proves ability; failing to compile proves nothing about ability in general (the
//! fixture set may simply contain no grammar with that construct, or every such grammar may trip an
//! unrelated budget). So the CANNOT-REPRESENT half stays declarative, read from
//! `crate::strategy_coverage::representation_of`, and `CompletenessReport` classifies every pair
//! that is neither collected nor declared as a GAP rather than as a negative result.
//!
//! # What a witness does and does not establish
//! It establishes exactly "this backend compiled a grammar containing this construct" — the claim
//! `crate::coverage_ledger`'s citations were making informally. It does NOT establish that the
//! backend's proposer represents the construct faithfully; a compile can succeed while the emitter
//! silently skips the construct's material, which is precisely the hole
//! `crate::strategy_coverage`'s `CannotRepresent` rows record. The two therefore CAN disagree, and
//! `CompletenessReport::contradictions` reports that overlap instead of hiding it behind whichever
//! side was consulted first.
//!
//! # Denominator, always
//! A coverage number is meaningless without what it ranged over, and the failure mode is a green
//! "covered everything" produced by one fixture and one backend. `CompletenessReport` therefore
//! carries the claimed conformance scope, the fixture counts, and the backends that actually
//! compiled something, and `CompletenessReport::render` prints all of it above the totals.

use std::collections::BTreeSet;
use std::panic::{self, AssertUnwindSafe};

use foma::options::FomaOptions;
use pg_grammar::model::Grammar;

use crate::analyzer::FomaProposer;
use crate::backend_selection::{select_backends, BackendReport};
use crate::capability::CharacteristicKind;
use crate::coverage_seam::{self, MeasuredOutcome, NotAttemptedReason, Verdict};
use crate::emit::surface_table;
use crate::enumerate::{enumerate_default, EmissionStrategy};
use crate::grammar_semantics::GrammarSemantics;
use crate::junctions::PhonologyProbe;
use crate::lowering_adapter::LoweringAdapter;
use crate::replace::SegAlphabet;
use crate::strategy_coverage::{representation_of, StrategyRepresentation, ALL_STRATEGIES};

/// What happened when one backend met one grammar. Only `Self::Compiled` can witness anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendOutcome {
    /// The backend's own compile entry point returned `Ok` on this grammar.
    Compiled,
    /// `crate::backend_selection` reported `crate::capability::CompileDecision::Refuse`, so this
    /// backend cannot legally run this grammar and no compile was attempted.
    RefusedBySelector,
    /// The backend was selected and its compile still failed or panicked; carries the reason.
    CompileFailed(String),
}

impl BackendOutcome {
    pub fn compiled(&self) -> bool {
        matches!(self, Self::Compiled)
    }

    /// Stable identifier for reports.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Compiled => "compiled",
            Self::RefusedBySelector => "refused-by-selector",
            Self::CompileFailed(_) => "compile-failed",
        }
    }
}

/// The compile-failure reason a `Self::CompileFailed` carries, recovered losslessly.
impl MeasuredOutcome for BackendOutcome {
    type Failure = String;

    fn classify(&self) -> Verdict<String> {
        match self {
            Self::Compiled => Verdict::Held,
            Self::CompileFailed(reason) => Verdict::Failed(reason.clone()),
            Self::RefusedBySelector => Verdict::NotAttempted(NotAttemptedReason::RefusedBySelector),
        }
    }
}

/// One grammar's contribution to the collection: the constructs it contains, and what each backend
/// did with it.
pub type GrammarObservation = coverage_seam::Observation<BackendOutcome>;

/// Compiles `g` with `strategy`'s REAL entry point — the only source of a witness in this module.
///
/// Each arm is the same entry point `crate::backend_runtime`'s own per-adapter realization uses, so
/// a witness collected here names a compiler the runtime can actually run:
/// `crate::analyzer::FomaProposer::new` for the surface probe,
/// `crate::templated_compile::compile_templated_morphotactics` for the templated cascade, and
/// `crate::build::build_controllable` plus the mandatory
/// `crate::build::finish_controllable_net` for the plan composer. Both whole-grammar backends
/// derive their own topology and take no plan, exactly as `crate::enumerate::EmissionStrategy`'s
/// own doc describes.
pub fn compile_with_backend(g: &Grammar, strategy: EmissionStrategy) -> Result<(), String> {
    match LoweringAdapter::for_strategy(strategy) {
        LoweringAdapter::TunedSurfaceEmit => FomaProposer::new(g)
            .map(|_| ())
            .map_err(|e| format!("tuned surface emit failed to build: {e}")),
        LoweringAdapter::TemplatedUnderlyingEmit => {
            crate::templated_compile::compile_templated_morphotactics(g)
                .map(|_| ())
                .map_err(|e| format!("templated underlying-token path failed to build: {e}"))
        }
        LoweringAdapter::ControllablePlanCompose => compile_plan_composed(g),
    }
}

/// The plan-composing backend's production shape: enumerate the default plan, interpret it, and run the mandatory boundary-token cleanup a proposer needs.
fn compile_plan_composed(g: &Grammar) -> Result<(), String> {
    let table = g
        .char_tables
        .first()
        .ok_or_else(|| "grammar has no character table".to_string())?;
    let alphabet = SegAlphabet::new(table);
    let semantics = GrammarSemantics::derive(g);
    // uflexc skips every Realizational rule wholesale, so a network can build while silently missing it.
    if semantics
        .characteristics()
        .observations()
        .iter()
        .any(|o| o.kind == CharacteristicKind::RealizationalMorphology)
    {
        let row = crate::strategy_coverage::representation_of(
            EmissionStrategy::PlanComposed,
            CharacteristicKind::RealizationalMorphology,
        );
        if row.representation == crate::strategy_coverage::StrategyRepresentation::CannotRepresent {
            return Err(format!(
                "plan-composed cannot honour a grammar exercising RealizationalMorphology: {}",
                row.evidence
            ));
        }
    }
    let phonology = PhonologyProbe::new_with_semantics(&semantics);
    let plan = enumerate_default(g, semantics.prules_in_order(), phonology.as_ref());
    let markers = crate::build::unbuildable_markers(&plan);
    if !markers.is_empty() {
        return Err(format!(
            "plan-composed cannot honour this plan: it requires subtrees build_controllable does \
             not build ({})",
            markers
                .iter()
                .map(|marker| format!("{marker:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let opts = FomaOptions::default();
    let mut built =
        crate::build::build_controllable(&plan, &opts, g, &alphabet, semantics.prules_in_order())
            .map_err(|e| format!("plan-composed build failed: {e:?}"))?;
    let net = built
        .net
        .take()
        .ok_or_else(|| "plan-composed build produced no network".to_string())?;
    crate::build::finish_controllable_net(&opts, net, surface_table(g), &alphabet);
    Ok(())
}

/// Characterizes `g`, asks `crate::backend_selection` which backends may run it, and COMPILES with
/// each one that may.
///
/// A backend whose report is `Refuse` records `BackendOutcome::RefusedBySelector` and is never
/// compiled — it cannot legally run this grammar, so a compile there would measure nothing the
/// selector permits. Every other backend is compiled for real, and only an `Ok` becomes a witness.
pub fn observe_grammar(label: &str, g: &Grammar) -> GrammarObservation {
    observe_grammar_with(label, g, &compile_with_backend)
}

/// `observe_grammar` with the compile step injected, so a test can force a backend to fail and
/// check that its witnesses disappear. The production caller passes `compile_with_backend`; nothing
/// else in this crate substitutes anything.
pub fn observe_grammar_with(
    label: &str,
    g: &Grammar,
    compile: &dyn Fn(&Grammar, EmissionStrategy) -> Result<(), String>,
) -> GrammarObservation {
    let semantics = GrammarSemantics::derive(g);
    let observed: BTreeSet<CharacteristicKind> = semantics
        .characteristics()
        .observations()
        .iter()
        .map(|o| o.kind)
        .collect();
    let kinds: Vec<CharacteristicKind> = CharacteristicKind::ALL
        .iter()
        .copied()
        .filter(|kind| observed.contains(kind))
        .collect();
    let selection = select_backends(&semantics);

    let outcomes = ALL_STRATEGIES
        .iter()
        .copied()
        .map(|strategy| {
            let representable = selection
                .report_for(strategy)
                .is_some_and(BackendReport::can_represent);
            if !representable {
                return (strategy, BackendOutcome::RefusedBySelector);
            }
            // A compiler contract violation panics rather than returning Err; a crash must not lose the whole sweep, and it is never a witness either way.
            let outcome = match panic::catch_unwind(AssertUnwindSafe(|| compile(g, strategy))) {
                Ok(Ok(())) => BackendOutcome::Compiled,
                Ok(Err(reason)) => BackendOutcome::CompileFailed(reason),
                Err(payload) => BackendOutcome::CompileFailed(format!(
                    "panicked: {}",
                    panic_message(payload.as_ref())
                )),
            };
            (strategy, outcome)
        })
        .collect();

    GrammarObservation {
        label: label.to_string(),
        kinds,
        outcomes,
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// How strictly `CompletenessReport::check` reads the gap inventory.
///
/// The strict reading is written and exercised today but is not what the gate asserts: making it
/// build-breaking before any gap is closed would turn `main` red for every unrelated change, and a
/// gate that blocks all work gets switched off. Flipping the gate is a one-word edit at its call
/// site, deliberately, so nothing has to be rebuilt when the inventory reaches zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletenessRequirement {
    /// The run must have measured something: at least one grammar, at least one witnessed pair, and
    /// at least two distinct backends compiling. Gaps are reported, never failed on.
    NonVacuity,
    /// `Self::NonVacuity` plus an empty gap inventory. The eventual gate.
    NoGaps,
}

/// The full account over `CharacteristicKind::ALL` x `crate::strategy_coverage::ALL_STRATEGIES`,
/// stated alongside the denominator it was collected over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletenessReport {
    /// What `pg_conformance_fixtures`'s scope variable claimed for this run, verbatim.
    pub scope: String,
    /// Grammars the caller discovered, before any of them was loaded.
    pub grammars_discovered: usize,
    /// Grammars that loaded and were characterized — the real denominator of the collection.
    pub grammars_observed: Vec<String>,
    /// Every kind at least one observed grammar contains. A kind absent here CANNOT be witnessed by
    /// this run at all, whatever the backends can do.
    pub kinds_exhibited: Vec<CharacteristicKind>,
    /// Backends that compiled at least one grammar. A one-backend run is the trap this field exists
    /// to expose.
    pub backends_compiling: Vec<EmissionStrategy>,
    /// Pairs collected by a real compile.
    pub witnessed: Vec<(CharacteristicKind, EmissionStrategy)>,
    /// Pairs `crate::strategy_coverage` declares the backend cannot represent. Declarative by
    /// necessity: see this module's own doc.
    pub declared_cannot_represent: Vec<(CharacteristicKind, EmissionStrategy)>,
    /// Pairs that are BOTH — a backend declared unable to represent the construct nonetheless
    /// compiled a grammar containing it. Not an error: compiling is not representing.
    pub contradictions: Vec<(CharacteristicKind, EmissionStrategy)>,
    /// Pairs that are neither collected nor declared: the actionable inventory.
    pub gaps: Vec<(CharacteristicKind, EmissionStrategy)>,
    /// Why each `Self::gaps` entry is a gap — no grammar exhibits the construct at all, or the ones
    /// that do were refused or failed on that backend. Without this the inventory names a number
    /// rather than a next step.
    pub gap_attributions: Vec<(CharacteristicKind, EmissionStrategy, String)>,
    /// Every compile that was attempted and did not succeed, as `(grammar, backend, reason)`.
    pub compile_failures: Vec<(String, EmissionStrategy, String)>,
    /// Every `(grammar, backend)` the selector refused before any compile.
    pub selector_refusals: Vec<(String, EmissionStrategy)>,
}

impl CompletenessReport {
    /// Every `(kind, backend)` pair the account ranges over.
    pub fn total_pairs() -> usize {
        CharacteristicKind::ALL.len() * ALL_STRATEGIES.len()
    }

    /// Pairs witnessed by a real compile with `strategy`.
    pub fn witnessed_for(&self, strategy: EmissionStrategy) -> Vec<CharacteristicKind> {
        self.witnessed
            .iter()
            .filter(|(_, s)| *s == strategy)
            .map(|(kind, _)| *kind)
            .collect()
    }

    /// Gaps attributed to `strategy`.
    pub fn gaps_for(&self, strategy: EmissionStrategy) -> Vec<CharacteristicKind> {
        self.gaps
            .iter()
            .filter(|(_, s)| *s == strategy)
            .map(|(kind, _)| *kind)
            .collect()
    }

    /// The requirement's verdict: `Ok` or every violated clause, named.
    pub fn check(&self, requirement: CompletenessRequirement) -> Result<(), Vec<String>> {
        let mut violations = Vec::new();
        if self.grammars_observed.is_empty() {
            violations.push(format!(
                "no grammar was observed at all (scope={}, discovered={})",
                self.scope, self.grammars_discovered
            ));
        }
        if self.witnessed.is_empty() {
            violations.push(
                "no (kind, backend) pair was witnessed by a real compile -- the collection ran but \
                 measured nothing"
                    .to_string(),
            );
        }
        if self.backends_compiling.len() < 2 {
            violations.push(format!(
                "only {} backend(s) compiled anything ({:?}) -- a single-backend run cannot \
                 distinguish one compiler's ability from three",
                self.backends_compiling.len(),
                self.backends_compiling
                    .iter()
                    .map(|s| s.label())
                    .collect::<Vec<_>>()
            ));
        }
        if requirement == CompletenessRequirement::NoGaps && !self.gaps.is_empty() {
            violations.push(format!(
                "{} (kind, backend) pair(s) are neither witnessed by a compile nor declared \
                 cannot-represent",
                self.gaps.len()
            ));
        }
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }

    /// The human-readable account: denominator first, then the totals, then the full gap inventory.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("witnessed-strategy-coverage\n");
        out.push_str("=== denominator ===\n");
        out.push_str(&format!("conformance scope claimed: {}\n", self.scope));
        out.push_str(&format!(
            "grammars discovered: {}; observed (loaded + characterized): {}\n",
            self.grammars_discovered,
            self.grammars_observed.len()
        ));
        out.push_str(&format!(
            "backends that compiled at least one grammar: {}\n",
            if self.backends_compiling.is_empty() {
                "NONE".to_string()
            } else {
                self.backends_compiling
                    .iter()
                    .map(|s| s.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));
        out.push_str(&format!(
            "constructs exhibited by at least one observed grammar: {} of {}\n",
            self.kinds_exhibited.len(),
            CharacteristicKind::ALL.len()
        ));
        let unexhibited: Vec<String> = CharacteristicKind::ALL
            .iter()
            .filter(|kind| !self.kinds_exhibited.contains(kind))
            .map(|kind| format!("{kind:?}"))
            .collect();
        out.push_str(&format!(
            "constructs NO observed grammar exhibits (unwitnessable by this run): {}\n",
            if unexhibited.is_empty() {
                "none".to_string()
            } else {
                unexhibited.join(", ")
            }
        ));

        out.push_str("=== totals ===\n");
        out.push_str(&format!(
            "pairs: {} total; {} witnessed by a real compile; {} declared cannot-represent; {} \
             both; {} gaps\n",
            Self::total_pairs(),
            self.witnessed.len(),
            self.declared_cannot_represent.len(),
            self.contradictions.len(),
            self.gaps.len()
        ));
        for &strategy in ALL_STRATEGIES {
            out.push_str(&format!(
                "  {}: {} witnessed / {} declared cannot-represent / {} gaps (of {} constructs)\n",
                strategy.label(),
                self.witnessed_for(strategy).len(),
                self.declared_cannot_represent
                    .iter()
                    .filter(|(_, s)| *s == strategy)
                    .count(),
                self.gaps_for(strategy).len(),
                CharacteristicKind::ALL.len()
            ));
        }

        out.push_str("=== gap inventory ===\n");
        if self.gaps.is_empty() {
            out.push_str("(none)\n");
        }
        for (kind, strategy, why) in &self.gap_attributions {
            out.push_str(&format!(
                "  GAP {kind:?} x {}\n       {why}\n",
                strategy.label()
            ));
        }

        if !self.contradictions.is_empty() {
            out.push_str("=== declared cannot-represent, yet compiled a grammar with it ===\n");
            for (kind, strategy) in &self.contradictions {
                out.push_str(&format!("  {kind:?} x {}\n", strategy.label()));
            }
        }

        if !self.compile_failures.is_empty() {
            out.push_str(&format!(
                "=== compile failures ({}) ===\n",
                self.compile_failures.len()
            ));
            for (label, strategy, reason) in &self.compile_failures {
                out.push_str(&format!("  {label} x {}: {reason}\n", strategy.label()));
            }
        }

        if !self.selector_refusals.is_empty() {
            out.push_str(&format!(
                "=== selector refusals ({}, never compiled) ===\n",
                self.selector_refusals.len()
            ));
            for (label, strategy) in &self.selector_refusals {
                out.push_str(&format!("  {label} x {}\n", strategy.label()));
            }
        }
        out
    }
}

/// Folds observations into the account, over the shared `crate::coverage_seam::build_report` fold
/// for the witnessed/held axis. `scope` and `grammars_discovered` are the caller's denominator
/// claim: this function cannot see what the caller chose to walk, so it never invents either.
///
/// `declared_cannot_represent`/`contradictions`/`gaps` are NOT part of the shared fold: they range
/// over the FULL `CharacteristicKind::ALL` grid (including a kind no observed grammar exhibits at
/// all, which the shared fold -- by design, see its own doc -- has no evidence for and so never
/// visits), and they compare against `crate::strategy_coverage::representation_of`'s declarative
/// table, a fact the shared fold has no vocabulary for either. `backends_compiling` also stays its
/// own pass: it credits only an actual `Compiled` outcome, never a `CompileFailed` one, which is a
/// narrower criterion than the shared fold's "attempted" (Held or Failed) reading of "active".
pub fn build_report(
    scope: &str,
    grammars_discovered: usize,
    observations: &[GrammarObservation],
) -> CompletenessReport {
    let matrix = coverage_seam::build_report(scope, grammars_discovered, observations);

    let mut backends_compiling_set: BTreeSet<usize> = BTreeSet::new();
    let mut compile_failures = Vec::new();
    let mut selector_refusals = Vec::new();
    for observation in observations {
        for (strategy, outcome) in &observation.outcomes {
            match outcome {
                BackendOutcome::Compiled => {
                    backends_compiling_set.insert(coverage_seam::strategy_index(*strategy));
                }
                BackendOutcome::CompileFailed(reason) => {
                    compile_failures.push((observation.label.clone(), *strategy, reason.clone()))
                }
                BackendOutcome::RefusedBySelector => {
                    selector_refusals.push((observation.label.clone(), *strategy));
                }
            }
        }
    }

    let mut witnessed = Vec::new();
    let mut declared_cannot_represent = Vec::new();
    let mut contradictions = Vec::new();
    let mut gaps = Vec::new();
    let mut gap_attributions = Vec::new();
    for &kind in CharacteristicKind::ALL {
        for &strategy in ALL_STRATEGIES {
            let is_witnessed = matrix.held.contains(&(kind, strategy));
            let is_declared = representation_of(strategy, kind).representation
                == StrategyRepresentation::CannotRepresent;
            if is_witnessed {
                witnessed.push((kind, strategy));
            }
            if is_declared {
                declared_cannot_represent.push((kind, strategy));
            }
            if is_witnessed && is_declared {
                contradictions.push((kind, strategy));
            }
            if !is_witnessed && !is_declared {
                gaps.push((kind, strategy));
                gap_attributions.push((
                    kind,
                    strategy,
                    attribute_gap(kind, strategy, observations),
                ));
            }
        }
    }

    CompletenessReport {
        scope: matrix.scope,
        grammars_discovered: matrix.discovered,
        grammars_observed: matrix.observed,
        kinds_exhibited: matrix.kinds_exhibited,
        backends_compiling: backends_compiling_set
            .iter()
            .map(|&index| ALL_STRATEGIES[index])
            .collect(),
        witnessed,
        declared_cannot_represent,
        contradictions,
        gaps,
        gap_attributions,
        compile_failures,
        selector_refusals,
    }
}

/// Names the next step for one gap: whether the run could have witnessed it at all, and if it could, what stopped each candidate grammar.
fn attribute_gap(
    kind: CharacteristicKind,
    strategy: EmissionStrategy,
    observations: &[GrammarObservation],
) -> String {
    let exhibiting: Vec<&GrammarObservation> = observations
        .iter()
        .filter(|observation| observation.kinds.contains(&kind))
        .collect();
    if exhibiting.is_empty() {
        return "no observed grammar exhibits this construct -- unwitnessable by this fixture set, \
                whatever the backend can do"
            .to_string();
    }
    let mut refused = 0usize;
    let mut failed = 0usize;
    let mut example = String::new();
    for observation in &exhibiting {
        match observation.outcome_for(strategy) {
            Some(BackendOutcome::RefusedBySelector) => {
                refused += 1;
                if example.is_empty() {
                    example = format!("{} (refused)", observation.label);
                }
            }
            Some(BackendOutcome::CompileFailed(reason)) => {
                failed += 1;
                if example.is_empty() {
                    example = format!("{} ({reason})", observation.label);
                }
            }
            Some(BackendOutcome::Compiled) | None => {}
        }
    }
    format!(
        "{} grammar(s) exhibit it; on this backend {refused} were refused by the selector and \
         {failed} failed to compile -- e.g. {example}",
        exhibiting.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(
        label: &str,
        kinds: &[CharacteristicKind],
        outcomes: &[(EmissionStrategy, BackendOutcome)],
    ) -> GrammarObservation {
        GrammarObservation {
            label: label.to_string(),
            kinds: kinds.to_vec(),
            outcomes: outcomes.to_vec(),
        }
    }

    /// A refused backend and a failed compile must both credit nothing, or the collector is just the declarative table with extra steps.
    #[test]
    fn only_a_successful_compile_witnesses_anything() {
        let report = build_report(
            "all",
            1,
            &[observation(
                "synthetic",
                &[CharacteristicKind::Affixation],
                &[
                    (EmissionStrategy::PlanComposed, BackendOutcome::Compiled),
                    (
                        EmissionStrategy::TunedSurfaceProbed,
                        BackendOutcome::CompileFailed("forced".to_string()),
                    ),
                    (
                        EmissionStrategy::TemplatedUnderlyingTokens,
                        BackendOutcome::RefusedBySelector,
                    ),
                ],
            )],
        );
        assert_eq!(
            report.witnessed,
            vec![(
                CharacteristicKind::Affixation,
                EmissionStrategy::PlanComposed
            )]
        );
        assert_eq!(
            report.backends_compiling,
            vec![EmissionStrategy::PlanComposed]
        );
        assert_eq!(report.compile_failures.len(), 1);
        assert_eq!(report.selector_refusals.len(), 1);
    }

    /// A kind no observed grammar contains can never be witnessed, however many backends compiled.
    #[test]
    fn a_kind_no_grammar_exhibits_is_never_witnessed() {
        let report = build_report(
            "all",
            1,
            &[observation(
                "synthetic",
                &[CharacteristicKind::Affixation],
                &ALL_STRATEGIES
                    .iter()
                    .map(|&s| (s, BackendOutcome::Compiled))
                    .collect::<Vec<_>>(),
            )],
        );
        for &kind in CharacteristicKind::ALL {
            if kind == CharacteristicKind::Affixation {
                continue;
            }
            assert!(
                !report.witnessed.iter().any(|(k, _)| *k == kind),
                "{kind:?} was witnessed by a grammar that does not contain it"
            );
        }
        assert_eq!(report.kinds_exhibited, vec![CharacteristicKind::Affixation]);
    }

    /// The three classes plus the declared/witnessed overlap must account for every pair exactly once.
    #[test]
    fn every_pair_is_witnessed_declared_or_a_gap() {
        let report = build_report(
            "all",
            1,
            &[observation(
                "synthetic",
                CharacteristicKind::ALL,
                &ALL_STRATEGIES
                    .iter()
                    .map(|&s| (s, BackendOutcome::Compiled))
                    .collect::<Vec<_>>(),
            )],
        );
        let union = report.witnessed.len() + report.declared_cannot_represent.len()
            - report.contradictions.len();
        assert_eq!(union + report.gaps.len(), CompletenessReport::total_pairs());
    }

    /// Every gap must carry its own reason, and the two reasons must be told apart: an unexhibited construct is a fixture-set limit, a refused one is a backend limit.
    #[test]
    fn each_gap_names_why_it_is_one() {
        let report = build_report(
            "all",
            1,
            &[observation(
                "synthetic",
                &[CharacteristicKind::Affixation],
                &[
                    (EmissionStrategy::PlanComposed, BackendOutcome::Compiled),
                    (
                        EmissionStrategy::TunedSurfaceProbed,
                        BackendOutcome::RefusedBySelector,
                    ),
                    (
                        EmissionStrategy::TemplatedUnderlyingTokens,
                        BackendOutcome::Compiled,
                    ),
                ],
            )],
        );
        assert_eq!(report.gaps.len(), report.gap_attributions.len());
        let affixation = report
            .gap_attributions
            .iter()
            .find(|(kind, strategy, _)| {
                *kind == CharacteristicKind::Affixation
                    && *strategy == EmissionStrategy::TunedSurfaceProbed
            })
            .expect("the refused backend must have an Affixation gap");
        assert!(
            affixation.2.contains("refused by the selector"),
            "{affixation:?}"
        );
        let unexhibited = report
            .gap_attributions
            .iter()
            .find(|(kind, _, _)| *kind == CharacteristicKind::Metathesis)
            .expect("a construct the fixture lacks must be a gap");
        assert!(
            unexhibited.2.contains("no observed grammar exhibits"),
            "{unexhibited:?}"
        );
    }

    /// An empty run must fail non-vacuity rather than report a clean sheet.
    #[test]
    fn a_vacuous_run_is_refused_by_the_non_vacuity_requirement() {
        let report = build_report("all", 0, &[]);
        let violations = report
            .check(CompletenessRequirement::NonVacuity)
            .expect_err("an empty collection must not pass");
        assert_eq!(violations.len(), 3, "{violations:?}");
        assert_eq!(report.gaps.len(), 66, "every representable pair is a gap");
    }

    /// The strict requirement is live code, not a comment: it must reject exactly what the lenient one tolerates.
    #[test]
    fn the_strict_requirement_rejects_a_gap_the_lenient_one_reports() {
        let mut outcomes: Vec<(EmissionStrategy, BackendOutcome)> = ALL_STRATEGIES
            .iter()
            .map(|&s| (s, BackendOutcome::Compiled))
            .collect();
        outcomes.pop();
        let report = build_report(
            "all",
            1,
            &[observation("synthetic", CharacteristicKind::ALL, &outcomes)],
        );
        assert!(!report.gaps.is_empty());
        assert!(report.check(CompletenessRequirement::NonVacuity).is_ok());
        let violations = report
            .check(CompletenessRequirement::NoGaps)
            .expect_err("a non-empty gap inventory must fail the strict requirement");
        assert!(violations.iter().any(|v| v.contains("neither witnessed")));
    }
}
