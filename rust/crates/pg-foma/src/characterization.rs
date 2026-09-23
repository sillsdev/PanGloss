//! The cheap characterization pass: reports constructs, quantifier/alternative products, alpha
//! tuples, templates/slots, predicted emitted work, peeled/confirm-only expansion, and
//! unknown/unbounded work before a selected production backend build. Unknown cost is not itself a
//! MachineLimit or CannotRepresent verdict when construction is recall-preserving; any uncertainty
//! that could omit an analysis fails closed.
//!
//! # Consume, never remeasure (same discipline as `crate::health_evaluator`)
//! `characterization_findings` takes a `&Grammar`, derives ONE
//! `crate::grammar_semantics::GrammarSemantics` from it, and reads exactly two existing, already-
//! tested facts off it — never re-derives their logic itself. Memoized characteristics may construct
//! Foma networks for `Simultaneous` subrules; that is characterization work, not a selected production
//! backend build:
//! - `crate::grammar_semantics::GrammarSemantics::characteristics` — this crate's own one-time,
//!   exhaustive walk over every represented `pg_grammar::model::Grammar` construct variant. Its
//!   `crate::capability::CharacteristicsProfile::cardinality`
//!   (`crate::capability::GrammarCardinality`) and per-observation detail structs
//!   (`crate::capability::QuantifierPatternDetail::all_bounded`,
//!   `crate::capability::UnorderedStratumDetail::rule_count`) feed this module's
//!   cardinality findings verbatim — never re-walked from the grammar.
//! - `crate::backend_selection::best_case_across_backends` — an ADVISORY-ONLY, whole-grammar join
//!   over every backend's compatibility report (see that function's own doc), composing
//!   `characterize` with the predicate registry (`crate::capability::compose_envelope`/
//!   `crate::capability::default_registry`) into one final `crate::capability::CompileDecision`.
//!
//! # Two distinct axes this module keeps separate
//! - **Semantic uncertainty** (`semantic_uncertainty_finding`): [`crate::capability::CompileDecision::
//!   Refuse`] — at least one construct has no predicate-proven recall-preserving compilation path at
//!   all. This characterization walk cannot guarantee every HermitCrab analysis survives, so it reports a
//!   `CannotRepresent` finding naming every `crate::capability::CapabilityDiagnostic` the gate collected.
//! - **Cost uncertainty** (`cost_uncertainty_finding`, `unbounded_quantifier_findings`):
//!   `crate::capability::CompileDecision::ConfirmOnly` (a first-class, non-failure verdict:
//!   propose the superset, HermitCrab confirm prunes false positives) or a specific
//!   `crate::capability::QuantifierPatternDetail::all_bounded` occurrence marked `false`. Always
//!   `LargeMultiplier`, never `CannotRepresent` on its own (unknown cost is not itself a
//!   CannotRepresent verdict when construction is recall-preserving) — an actual budget trip during the real compile is a completely
//!   different, already-handled code path: `crate::health_evaluator::compose_error_finding`'s
//!   `crate::health::FindingCode::ResourceBudgetReached` arm, which this module's own findings
//!   never constructs.
//!
//! # Bounded products
//! `rule_interaction_product_finding` computes bounded products for alternatives, templates, and
//! slots: `crate::capability::GrammarCardinality::mrule_count` times `prule_count` is a cheap,
//! generic proxy for how much morphological x phonological rule-interaction surface a grammar
//! presents. `RULE_PRODUCT_WARNING_THRESHOLD` is a conservative, provisional placeholder (this
//! crate's own repeated convention): no real-grammar calibration evidence exists yet for this
//! specific product, so this finding is
//! `Predicted`/`LargeMultiplier` only, never something that can reject a compile on its own. It is
//! `crate::health::FindingCode::RuleInteractionProduct`, not `UnknownUnboundedConstruct`: the
//! product is an EXACT, already-computed count (large, not unknown), the textbook
//! `Severity::LargeMultiplier` case per that variant's own doc.
//!
//! # Design notes
//! 1. **Semantic and cost uncertainty use different stable codes.** A `Refuse` verdict is a known
//!    coverage gap and uses `crate::health::FindingCode::BackendCoverageIncomplete` with
//!    `crate::health::Metric::BackendCoverageGapCount`. Recall-preserving `ConfirmOnly` and
//!    unbounded quantifiers remain genuine cost uncertainty and use
//!    `crate::health::FindingCode::UnknownUnboundedConstruct`; the rule-interaction proxy is an
//!    exact bounded product instead and uses `crate::health::FindingCode::RuleInteractionProduct`.
//! 2. **`semantic_uncertainty_finding`'s `affected` names each [`crate::capability::
//!    CapabilityDiagnostic::construct`] string verbatim** — never a re-derived identifier
//!    scheme.
//! 3. **`rule_interaction_product_finding`'s `affected` is empty** — this is a grammar-wide
//!    cardinality fact, not about any one construct, matching `crate::health_evaluator`'s own
//!    "grammar-level findings with no specific construct identifier ... leave `affected` empty"
//!    convention (`payload_size_finding`).

use std::sync::Mutex;

use crate::backend_selection::best_case_across_backends;
use crate::capability::{CharacteristicsProfile, CompileDecision, ObservationDetail};
use crate::grammar_semantics::GrammarSemantics;
use crate::health::{
    FindingCode, HealthFinding, Metric, MetricValue, Phase, Severity, ValueProvenance,
};
use pg_grammar::model::Grammar;

pub use pg_foma_runtime::emit_report::{
    CharacterizationResult, ClosureEvidence, ClosureStopReason, ClosureTerminal,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosureTestLimits {
    #[cfg(feature = "test-support")]
    pub work_cap: usize,
    #[cfg(feature = "test-support")]
    pub depth_cap: usize,
    #[cfg(not(feature = "test-support"))]
    pub(crate) work_cap: usize,
    #[cfg(not(feature = "test-support"))]
    pub(crate) depth_cap: usize,
}

/// Default logical-work budget for TunedSurface composite closure. This counts reachable
/// root/chain-state x rule applications, never affix depth. Ordinary selection keeps this budget
/// frozen; characterization uses these fixed internal limits.
pub(crate) const DEFAULT_TUNED_CLOSURE_WORK_LIMIT: usize = 3_000;
pub(crate) const DEFAULT_TUNED_CLOSURE_DEPTH_LIMIT: usize = 64;
const DEFAULT_TUNED_COMPOUND_CHAIN_DEPTH_LIMIT: usize = 200;

/// Mutable evidence sink shared by the production emitter and characterization APIs.
pub(crate) struct ClosureTrace {
    limits: ClosureTestLimits,
    compound_chain_depth_cap: usize,
    state: Mutex<TraceState>,
}

#[derive(Debug, Clone)]
struct TraceState {
    rule_pairs_visited: usize,
    synthesized_successors: usize,
    maximum_depth: usize,
    per_depth_counts: Vec<usize>,
    pending_successor_count: usize,
    pending_rule_ordinals: Vec<u32>,
    stop: Option<ClosureStopReason>,
    terminal: Option<ClosureTerminal>,
}

impl ClosureTrace {
    pub(crate) fn new(limits: ClosureTestLimits) -> Self {
        Self {
            limits,
            compound_chain_depth_cap: DEFAULT_TUNED_COMPOUND_CHAIN_DEPTH_LIMIT,
            state: Mutex::new(TraceState {
                rule_pairs_visited: 0,
                synthesized_successors: 0,
                maximum_depth: 0,
                per_depth_counts: Vec::new(),
                pending_successor_count: 0,
                pending_rule_ordinals: Vec::new(),
                stop: None,
                terminal: None,
            }),
        }
    }

    pub(crate) fn depth_cap(&self) -> usize {
        self.limits.depth_cap
    }

    pub(crate) fn compound_chain_depth_cap(&self) -> usize {
        self.compound_chain_depth_cap
    }

    /// Records one legal production transition before synthesis. A false result tells the
    /// caller to stop without dropping the live transition at the work boundary.
    pub(crate) fn begin_pair(&self, depth: usize, ordinal: u32) -> bool {
        let mut state = self.state.lock().expect("closure trace mutex poisoned");
        if state.terminal.is_some() || state.stop.is_some() {
            return false;
        }
        state.maximum_depth = state.maximum_depth.max(depth);
        if state.rule_pairs_visited >= self.limits.work_cap {
            state.pending_successor_count = state.pending_successor_count.saturating_add(1);
            state.pending_rule_ordinals.push(ordinal);
            state.stop = Some(ClosureStopReason::WorkBudgetReached);
            return false;
        }
        state.rule_pairs_visited += 1;
        if state.per_depth_counts.len() <= depth {
            state.per_depth_counts.resize(depth + 1, 0);
        }
        state.per_depth_counts[depth] += 1;
        true
    }

    pub(crate) fn record_successors(&self, depth: usize, ordinal: u32, successors: usize) -> bool {
        let mut state = self.state.lock().expect("closure trace mutex poisoned");
        if state.terminal.is_some() || state.stop.is_some() {
            return false;
        }
        state.synthesized_successors = state.synthesized_successors.saturating_add(successors);
        if depth >= self.limits.depth_cap && successors != 0 {
            state.pending_successor_count =
                state.pending_successor_count.saturating_add(successors);
            state.pending_rule_ordinals.push(ordinal);
            state.stop = Some(ClosureStopReason::DepthBudgetReached);
            return false;
        }
        true
    }

    pub(crate) fn result(&self) -> CharacterizationResult {
        let mut state = self.state.lock().expect("closure trace mutex poisoned");
        state.pending_rule_ordinals.sort_unstable();
        state.pending_rule_ordinals.dedup();
        let worklist_empty =
            state.terminal.is_none() && state.stop.is_none() && state.pending_successor_count == 0;
        let terminal = match state.terminal {
            Some(terminal) => terminal,
            None => match state.stop {
                Some(reason) => ClosureTerminal::Incomplete(reason),
                None if worklist_empty => ClosureTerminal::Complete,
                None => ClosureTerminal::Incomplete(ClosureStopReason::InternalConstructionFault),
            },
        };
        CharacterizationResult {
            terminal,
            evidence: ClosureEvidence {
                rule_pairs_visited: state.rule_pairs_visited,
                synthesized_successors: state.synthesized_successors,
                maximum_depth: state.maximum_depth,
                per_depth_counts: state.per_depth_counts.clone(),
                pending_successor_count: state.pending_successor_count,
                pending_rule_ordinals: state.pending_rule_ordinals.clone(),
                worklist_empty,
            },
        }
    }

    pub(crate) fn refuse(&self, reason: ClosureStopReason) {
        let mut state = self.state.lock().expect("closure trace mutex poisoned");
        if state.terminal.is_none() {
            state.terminal = Some(ClosureTerminal::Refused(reason));
        }
    }

    pub(crate) fn stop(&self, reason: ClosureStopReason) {
        let mut state = self.state.lock().expect("closure trace mutex poisoned");
        if state.terminal.is_none() {
            state.terminal = Some(ClosureTerminal::Incomplete(reason));
        }
    }
}

/// A conservative, uncalibrated placeholder (see this module's doc, "Bounded products"); never used to reject a compile, `Predicted`/`LargeMultiplier` evidence only.
const RULE_PRODUCT_WARNING_THRESHOLD: u64 = 64;

/// The characterization walker: every `crate::health::HealthFinding` this crate can derive before a
/// selected production backend build, from `g` alone. See this module's own doc for the
/// semantic-vs-cost-uncertainty split and the bounded-product findings.
pub fn characterization_findings(g: &Grammar) -> Vec<HealthFinding> {
    characterization_findings_with_semantics(&GrammarSemantics::derive(g))
}

/// `characterization_findings` over an already-derived `GrammarSemantics` -- so a caller running
/// characterization alongside its own capability gate (`pangloss fst-health` is exactly such a caller)
/// characterizes once in total, not once per call site.
pub fn characterization_findings_with_semantics(
    semantics: &GrammarSemantics<'_>,
) -> Vec<HealthFinding> {
    // One derivation, shared, rather than a second characterize walk inside the join.
    let profile = semantics.characteristics();
    let decision = best_case_across_backends(semantics);

    let mut findings = Vec::new();
    findings.extend(semantic_uncertainty_finding(&decision));
    findings.extend(cost_uncertainty_finding(&decision));
    findings.extend(unbounded_quantifier_findings(profile));
    findings.extend(rule_interaction_product_finding(profile));
    findings
}

/// `CompileDecision::Refuse`: at least one construct has no predicate-proven recall-preserving compilation path. `None` for `Admit`/`ConfirmOnly`.
fn semantic_uncertainty_finding(decision: &CompileDecision) -> Option<HealthFinding> {
    let CompileDecision::Refuse(diags) = decision else {
        return None;
    };
    let affected: Vec<String> = diags.iter().map(|d| d.construct.clone()).collect();
    let witnesses: Vec<String> = diags
        .iter()
        .map(|d| {
            format!(
                "predicate={} construct={} witness={}",
                d.predicate, d.construct, d.witness
            )
        })
        .collect();
    Some(HealthFinding::new(
        FindingCode::BackendCoverageIncomplete,
        Severity::CannotRepresent,
        Phase::Characterization,
        Metric::BackendCoverageGapCount,
        MetricValue::Count(diags.len() as u64),
        ValueProvenance::Observed,
        format!(
            "This grammar's ADR 0001 capability gate resolves to Refuse: {} construct(s) have no \
             predicate-proven recall-preserving compilation path ({}), so this characterization walk cannot \
             guarantee every HermitCrab analysis would be retained. R6: any uncertainty that could \
             omit an analysis fails closed. An explicit ADR 0005 capability override can force \
             compilation anyway.",
            diags.len(),
            witnesses.join("; "),
        ),
    )
        .affecting(affected))
}

/// `CompileDecision::ConfirmOnly`: recall-preserving, but this characterization stage has no proven cost bound for the construct(s) that landed here. Always `LargeMultiplier`/`Predicted`; `None` for `Admit`/`Refuse`.
fn cost_uncertainty_finding(decision: &CompileDecision) -> Option<HealthFinding> {
    if !matches!(decision, CompileDecision::ConfirmOnly) {
        return None;
    }
    Some(HealthFinding::new(
        FindingCode::UnknownUnboundedConstruct,
        Severity::LargeMultiplier,
        Phase::Characterization,
        Metric::UnknownUnboundedWork,
        MetricValue::Unbounded,
        ValueProvenance::Predicted,
        "This grammar's ADR 0001 capability gate resolves to ConfirmOnly: at least \
             one construct rests at a config-predicate-resolved recall-preserving disposition \
             (propose the superset, HermitCrab confirm prunes false positives), but this characterization \
             stage has no proven bound on the FST-compile cost it adds. Not itself a CannotRepresent verdict \
             (R6: unknown cost in a recall-preserving construction); a recall-preserving compilation \
             attempt is permitted under the fixed internal limits."
            .to_string(),
    ))
}

/// One `Predicted`/`LargeMultiplier` finding per rule with a genuinely unbounded (`max="-1"`) quantifier occurrence.
fn unbounded_quantifier_findings(profile: &CharacteristicsProfile) -> Vec<HealthFinding> {
    profile
        .observations()
        .iter()
        .filter_map(|o| match &o.detail {
            ObservationDetail::QuantifierPattern(d) if !d.all_bounded => Some(HealthFinding::new(
                FindingCode::UnknownUnboundedConstruct,
                Severity::LargeMultiplier,
                Phase::Characterization,
                Metric::UnknownUnboundedWork,
                MetricValue::Unbounded,
                ValueProvenance::Predicted,
                format!(
                    "Rule {:?} has at least one quantifier occurrence with no concrete max bound \
                     (the DTD's max=\"-1\" Kleene sentinel); this characterization stage cannot bound the \
                     FST-compile cost this rule adds ahead of time. Not itself a CannotRepresent verdict (R6): a \
                     recall-preserving compilation attempt is permitted under the fixed internal \
                     limits.",
                    d.rule,
                ),
            )
                .affecting(vec![format!("{:?}", d.rule)])),
            _ => None,
        })
        .collect()
}

/// The grammar-wide bounded-product case (module doc, "Bounded products"); `None` at or below `RULE_PRODUCT_WARNING_THRESHOLD`.
fn rule_interaction_product_finding(profile: &CharacteristicsProfile) -> Option<HealthFinding> {
    let mrule_count = profile.cardinality.mrule_count as u64;
    let prule_count = profile.cardinality.prule_count as u64;
    let product = mrule_count.saturating_mul(prule_count);
    if product <= RULE_PRODUCT_WARNING_THRESHOLD {
        return None;
    }
    Some(HealthFinding::new(
        FindingCode::RuleInteractionProduct,
        Severity::LargeMultiplier,
        Phase::Characterization,
        Metric::UnknownUnboundedWork,
        MetricValue::Count(product),
        ValueProvenance::Predicted,
        format!(
            "This grammar has {mrule_count} morphological rule(s) and {prule_count} phonological \
             rule(s) ({mrule_count} x {prule_count} = {product}), above this characterization stage's \
             conservative {RULE_PRODUCT_WARNING_THRESHOLD}-product warning band. This is a cheap, \
             generic proxy for morphological x phonological rule-interaction surface, not an exact \
             compile-work count; consider whether constraining or decomposing one of the two rule \
             sets reduces their multiplicative interaction."
        ),
    )
        .against_threshold(MetricValue::Count(RULE_PRODUCT_WARNING_THRESHOLD)))
}

#[cfg(test)]
mod tests;
