//! `openspec/changes/add-fst-compilation-health-audit`, tasks.md section 1 ("Preflight"): the
//! cheap, pre-compile health pass this change's own `design.md` describes ("Preflight reports
//! constructs, quantifier/alternative products, alpha tuples, templates/slots, predicted emitted
//! work, peeled/confirm-only expansion, and unknown/unbounded work without foma") and
//! `IMPLEMENTATION-READINESS.md` R6 asks for ("Unknown cost is not itself Critical when
//! construction is recall-preserving ... Any uncertainty that could omit an analysis fails
//! closed").
//!
//! # Consume, never remeasure (R6, same discipline as `crate::health_evaluator`)
//! [`preflight_findings`] takes a `&Grammar`, derives ONE
//! [`crate::grammar_semantics::GrammarSemantics`] from it, and reads exactly two existing, already-
//! tested, pure-Rust (no foma, no I/O) facts off it — never re-derives their logic itself:
//! - [`crate::grammar_semantics::GrammarSemantics::characteristics`] — this crate's own one-time,
//!   exhaustive walk over every represented `pg_grammar::model::Grammar` construct variant
//!   (`openspec/changes/add-capability-characteristics-check`). Its
//!   [`crate::capability::CharacteristicsProfile::cardinality`]
//!   ([`crate::capability::GrammarCardinality`]) and per-observation detail structs
//!   ([`crate::capability::QuantifierPatternDetail::all_bounded`],
//!   [`crate::capability::UnorderedStratumDetail::rule_count`]/`within_bound`) feed this module's
//!   cardinality/bounded-product findings verbatim — never re-walked from the grammar.
//! - [`crate::capability_entry::evaluate_capability_with_semantics`] — the SAME ADR 0001
//!   capability-gate entry point `pg-cli`'s own `run_capability_gate`/`pangloss pack` already call
//!   (through its `&Grammar` front end `evaluate_capability`), composing
//!   `characterize` with the predicate registry
//!   ([`crate::capability::compose_envelope`]/`crate::capability::default_registry`) into one final
//!   [`crate::capability::CompileDecision`]. This module reuses that FINAL, predicate-resolved
//!   verdict directly rather than re-implementing the D1 disposition/predicate-resolution logic
//!   itself — a raw, unresolved [`crate::capability::Disposition::ConfigPredicate`] characteristic
//!   (e.g. `Compounding`, `UnorderedMorphRuleApplication`, `QuantifierPattern`) only resolves to a
//!   real `ConfirmOnly`-vs-`Refuse` verdict THROUGH that predicate registry, so reasoning from raw
//!   per-kind dispositions alone (without running the registry) would misclassify most
//!   `ConfigPredicate` characteristics. **This used to be TWO `characterize` walks** — one here and
//!   a second inside `evaluate_capability`, which had no way to accept an already-built profile —
//!   and this doc called it "an acceptable duplication ... while waiting on 7.11". Task 7.11
//!   (`openspec/changes/cleanup-and-recipe-parity`) closed it: both now read the SAME memoized
//!   profile off one [`crate::grammar_semantics::GrammarSemantics`], so a `pangloss fst-health` run
//!   characterizes once rather than twice. The duplication was never as cheap as that note claimed,
//!   either — `characterize` builds real `foma::types::Fsm` networks for `Simultaneous`-mode
//!   subrules.
//!
//! # Two distinct axes this module keeps separate (task 1.3; spec.md's two preflight scenarios)
//! - **Semantic uncertainty** ([`semantic_uncertainty_finding`]): [`crate::capability::CompileDecision::
//!   Refuse`] — at least one construct has no predicate-proven recall-preserving compilation path at
//!   all. This preflight walk cannot guarantee every HermitCrab analysis survives, so it reports a
//!   `Critical` finding naming every [`crate::capability::CapabilityDiagnostic`] the gate collected —
//!   exactly spec.md's "rejects compilation with a typed semantic finding" scenario. This finding
//!   never itself blocks the actual compiler pass (it is evidence, not a second gate — `pg-cli`'s own
//!   `run_capability_gate`/`pangloss pack` are the real ADR 0001 enforcement points a caller consults
//!   separately); it is this report's own honest record of the same fact, consistent with (never
//!   duplicating the logic of) that gate.
//! - **Cost uncertainty** ([`cost_uncertainty_finding`], [`unbounded_quantifier_findings`]):
//!   [`crate::capability::CompileDecision::ConfirmOnly`] (ADR 0001's first-class, non-failure
//!   verdict: propose the superset, HermitCrab confirm prunes false positives) or a specific
//!   [`crate::capability::QuantifierPatternDetail::all_bounded`] occurrence marked `false`. Always
//!   `Warning`, never `Critical` on its own (R6: "Unknown cost is not itself Critical when
//!   construction is recall-preserving") — an actual budget trip during the real compile is a
//!   completely different, already-handled code path
//!   ([`crate::health_evaluator::compose_error_finding`]'s `ResourceBudgetReached`/
//!   `ProvenBoundExceedsBudget` arms), never reached from this module.
//!
//! # Bounded products (task 1.2)
//! [`unordered_stratum_findings`] reuses [`crate::capability::CharacteristicsProfile::
//! unordered_stratum_details`]'s ALREADY-COMPUTED `rule_count`/`within_bound` against the SAME
//! [`crate::compose_budget::DEFAULT_ORDERING_MULTIPLICITY_BUDGET`] the real compile-time check
//! (`crate::unordered::check_unordered_strata_bound`, surfaced post-hoc by
//! `crate::health_evaluator::compose_error_finding`'s `OrderingMultiplicityExceeded` arm) trips
//! against — the exact count is already known before foma ever runs, so this finding's
//! [`crate::health::ValueProvenance`] is `ProvenBound`, not a heuristic guess, and its severity is
//! `Critical` (an exact count proven to exceed budget, the same "proven lower bound" shape R6 says
//! may stop work before allocation). This deliberately duplicates part of what
//! [`semantic_uncertainty_finding`]'s `Refuse` case already names in a less specific way (a
//! grammar-wide `Refuse` diagnostic list) with a MORE specific, metric-tagged finding for this one
//! construct — both are kept, since [`crate::health::Metric::OrderingRuleCount`] carries information
//! (the exact rule count vs. the budget) the generic diagnostic-list finding does not.
//!
//! [`rule_interaction_product_finding`] is this module's own reading of design.md's "Calculate
//! bounded products for alternatives ... templates, and slots": [`crate::capability::
//! GrammarCardinality::mrule_count`] times `prule_count` is a cheap, generic proxy for how much
//! morphological x phonological rule-interaction surface a grammar presents (spec.md's own worked
//! scenario, "Alternatives multiply across two rules"). [`RULE_PRODUCT_WARNING_THRESHOLD`] is a
//! conservative, provisional placeholder (this crate's own repeated convention — mirrors
//! `crate::health_evaluator::APPROACHING_BUDGET_WARNING_FRACTION`'s identical disclaimer): no
//! real-grammar calibration evidence exists yet for this specific product, so this finding is
//! `Predicted`/`Warning` only, never something that can reject a compile on its own.
//!
//! # Judgment calls flagged for review
//! 1. **Every uncertainty finding reuses [`crate::health::FindingCode::UnknownUnboundedConstruct`]**,
//!    at different severities (`Critical` for `Refuse`, `Warning` for `ConfirmOnly`/unbounded
//!    quantifiers/the rule-interaction product) — the SAME "same code, severity carries the
//!    distinction" pattern `crate::health_evaluator`'s own `unsupported_tier_finding`/
//!    `partial_tier_finding` already established for `FomaTier::Unsupported` vs. `FomaTier::Partial`
//!    (that module's "Judgment calls" item 4, cited verbatim: "deliberately diverging from that
//!    code's general 'not itself Critical' framing"). No new `FindingCode` is minted for this
//!    additive step.
//! 2. **[`semantic_uncertainty_finding`]'s `affected` names each [`crate::capability::
//!    CapabilityDiagnostic::construct`] string verbatim** (the same field `pg-cli`'s own
//!    `run_capability_gate`/`pangloss pack` already print to stderr) — never a re-derived
//!    identifier scheme.
//! 3. **[`rule_interaction_product_finding`]'s `affected` is empty** — this is a grammar-wide
//!    cardinality fact, not about any one construct, matching `crate::health_evaluator`'s own
//!    "grammar-level findings with no specific construct identifier ... leave `affected` empty"
//!    convention (`payload_size_finding`).

use pg_grammar::model::Grammar;

use crate::capability::{CharacteristicsProfile, CompileDecision, ObservationDetail};
use crate::capability_entry::evaluate_capability_with_semantics;
use crate::compose_budget::DEFAULT_ORDERING_MULTIPLICITY_BUDGET;
use crate::grammar_semantics::GrammarSemantics;
use crate::health::{
    FindingCode, HealthFinding, Metric, MetricValue, Phase, Severity, ValueProvenance,
};

/// See this module's doc, "Bounded products" — a conservative, provisional placeholder (no
/// real-grammar calibration evidence exists yet for this specific `mrule_count * prule_count`
/// product). Never used to reject a compile; `Predicted`/`Warning` evidence only.
const RULE_PRODUCT_WARNING_THRESHOLD: u64 = 64;

/// The preflight walker (task deliverable 1; design.md "Preflight reports constructs ... without
/// foma"): every [`crate::health::HealthFinding`] this crate can derive BEFORE any foma compile is
/// attempted, from `g` alone. See this module's own doc for the semantic-vs-cost-uncertainty split
/// (task 1.3) and the bounded-product findings (task 1.2).
pub fn preflight_findings(g: &Grammar) -> Vec<HealthFinding> {
    preflight_findings_with_semantics(&GrammarSemantics::derive(g))
}

/// [`preflight_findings`] over an already-derived [`GrammarSemantics`] (task 7.11,
/// `openspec/changes/cleanup-and-recipe-parity`) -- so a caller running preflight alongside its own
/// capability gate (`pangloss fst-health` did exactly this, twice) characterizes once in total.
pub fn preflight_findings_with_semantics(semantics: &GrammarSemantics<'_>) -> Vec<HealthFinding> {
    // ONE derivation, shared. Before task 7.11 these were two independent `characterize` walks --
    // `capability::characterize(g)` here and a second one inside `evaluate_capability`, which this
    // module's own doc called "an acceptable duplication ... while waiting on 7.11".
    let profile = semantics.characteristics();
    let decision = evaluate_capability_with_semantics(semantics);

    let mut findings = Vec::new();
    findings.extend(semantic_uncertainty_finding(&decision));
    findings.extend(cost_uncertainty_finding(&decision));
    findings.extend(unbounded_quantifier_findings(profile));
    findings.extend(unordered_stratum_findings(profile));
    findings.extend(rule_interaction_product_finding(profile));
    findings
}

/// Task 1.3 / spec.md "A represented variant lacks a recall-preserving disposition":
/// [`CompileDecision::Refuse`] — this grammar's ADR 0001 capability gate has at least one construct
/// with no predicate-proven recall-preserving compilation path. `None` for `Admit`/`ConfirmOnly`.
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
    Some(HealthFinding {
        code: FindingCode::UnknownUnboundedConstruct,
        severity: Severity::Critical,
        phase: Phase::Preflight,
        affected,
        metric: Metric::UnknownUnboundedWork,
        value: MetricValue::Count(diags.len() as u64),
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation: format!(
            "This grammar's ADR 0001 capability gate resolves to Refuse: {} construct(s) have no \
             predicate-proven recall-preserving compilation path ({}), so this preflight walk cannot \
             guarantee every HermitCrab analysis would be retained. R6: any uncertainty that could \
             omit an analysis fails closed. An explicit ADR 0005 capability override can force \
             compilation anyway.",
            diags.len(),
            witnesses.join("; "),
        ),
        remedies: Vec::new(),
        override_record: None,
    })
}

/// Task 1.3 / spec.md "A represented variant lacks a cost model": [`CompileDecision::ConfirmOnly`]
/// — recall-preserving (the FST proposer proposes the superset; HermitCrab confirm prunes), but
/// this preflight stage has no proven cost bound for whichever construct(s) landed here. Always
/// `Warning`, `Predicted` (a pre-compile heuristic, not an exact count of compile work). `None` for
/// `Admit`/`Refuse`.
fn cost_uncertainty_finding(decision: &CompileDecision) -> Option<HealthFinding> {
    if !matches!(decision, CompileDecision::ConfirmOnly) {
        return None;
    }
    Some(HealthFinding {
        code: FindingCode::UnknownUnboundedConstruct,
        severity: Severity::Warning,
        phase: Phase::Preflight,
        affected: Vec::new(),
        metric: Metric::UnknownUnboundedWork,
        value: MetricValue::Unbounded,
        provenance: ValueProvenance::Predicted,
        threshold: None,
        explanation: "This grammar's ADR 0001 capability gate resolves to ConfirmOnly: at least \
             one construct rests at a config-predicate-resolved recall-preserving disposition \
             (propose the superset, HermitCrab confirm prunes false positives), but this preflight \
             stage has no proven bound on the FST-compile cost it adds. Not itself Critical (R6: \
             unknown cost in a recall-preserving construction); a recall-preserving compilation \
             attempt is permitted under the shared resource envelope."
            .to_string(),
        remedies: Vec::new(),
        override_record: None,
    })
}

/// Task 1.2's per-rule cost-uncertainty case: every [`crate::capability::QuantifierPatternDetail`]
/// observation whose `all_bounded` is `false` (a genuinely unbounded `max="-1"` quantifier
/// occurrence) — one finding per affected rule, `Predicted`/`Warning` (same reasoning as
/// [`cost_uncertainty_finding`]).
fn unbounded_quantifier_findings(profile: &CharacteristicsProfile) -> Vec<HealthFinding> {
    profile
        .observations()
        .iter()
        .filter_map(|o| match &o.detail {
            ObservationDetail::QuantifierPattern(d) if !d.all_bounded => Some(HealthFinding {
                code: FindingCode::UnknownUnboundedConstruct,
                severity: Severity::Warning,
                phase: Phase::Preflight,
                affected: vec![format!("{:?}", d.rule)],
                metric: Metric::UnknownUnboundedWork,
                value: MetricValue::Unbounded,
                provenance: ValueProvenance::Predicted,
                threshold: None,
                explanation: format!(
                    "Rule {:?} has at least one quantifier occurrence with no concrete max bound \
                     (the DTD's max=\"-1\" Kleene sentinel); this preflight stage cannot bound the \
                     FST-compile cost this rule adds ahead of time. Not itself Critical (R6): a \
                     recall-preserving compilation attempt is permitted under the shared resource \
                     envelope.",
                    d.rule,
                ),
                remedies: Vec::new(),
                override_record: None,
            }),
            _ => None,
        })
        .collect()
}

/// Task 1.2's bounded-product case for `MorphRuleOrder::Unordered` strata: reuses
/// [`crate::capability::CharacteristicsProfile::unordered_stratum_details`]'s ALREADY-COMPUTED
/// `rule_count`/`within_bound` (see this module's doc). `within_bound == false` means the exact
/// rule count is already proven, before foma runs, to exceed
/// [`DEFAULT_ORDERING_MULTIPLICITY_BUDGET`] — `ProvenBound`, `Critical`, the same "proven lower
/// bound can reject before allocation" shape R6 describes.
fn unordered_stratum_findings(profile: &CharacteristicsProfile) -> Vec<HealthFinding> {
    profile
        .unordered_stratum_details()
        .filter(|d| !d.within_bound)
        .map(|d| HealthFinding {
            code: FindingCode::ProvenBoundExceedsBudget,
            severity: Severity::Critical,
            phase: Phase::Preflight,
            affected: vec![format!("{:?}", d.stratum)],
            metric: Metric::OrderingRuleCount,
            value: MetricValue::Count(d.rule_count as u64),
            provenance: ValueProvenance::ProvenBound,
            threshold: Some(MetricValue::Count(DEFAULT_ORDERING_MULTIPLICITY_BUDGET as u64)),
            explanation: format!(
                "Unordered stratum {:?} has {} loose rules (limit {}), an exact count already known \
                 from this grammar's own shape before any foma compile begins, proven to exceed this \
                 grammar's ordering-multiplicity budget.",
                d.stratum, d.rule_count, DEFAULT_ORDERING_MULTIPLICITY_BUDGET,
            ),
            remedies: Vec::new(),
            override_record: None,
        })
        .collect()
}

/// Task 1.2's grammar-wide bounded-product case (spec.md "Alternatives multiply across two rules"):
/// see this module's doc "Bounded products". `None` when the product is at or below
/// [`RULE_PRODUCT_WARNING_THRESHOLD`].
fn rule_interaction_product_finding(profile: &CharacteristicsProfile) -> Option<HealthFinding> {
    let mrule_count = profile.cardinality.mrule_count as u64;
    let prule_count = profile.cardinality.prule_count as u64;
    let product = mrule_count.saturating_mul(prule_count);
    if product <= RULE_PRODUCT_WARNING_THRESHOLD {
        return None;
    }
    Some(HealthFinding {
        code: FindingCode::UnknownUnboundedConstruct,
        severity: Severity::Warning,
        phase: Phase::Preflight,
        affected: Vec::new(),
        metric: Metric::UnknownUnboundedWork,
        value: MetricValue::Count(product),
        provenance: ValueProvenance::Predicted,
        threshold: Some(MetricValue::Count(RULE_PRODUCT_WARNING_THRESHOLD)),
        explanation: format!(
            "This grammar has {mrule_count} morphological rule(s) and {prule_count} phonological \
             rule(s) ({mrule_count} x {prule_count} = {product}), above this preflight stage's \
             conservative {RULE_PRODUCT_WARNING_THRESHOLD}-product warning band. This is a cheap, \
             generic proxy for morphological x phonological rule-interaction surface, not an exact \
             compile-work count; consider whether constraining or decomposing one of the two rule \
             sets reduces their multiplicative interaction."
        ),
        remedies: Vec::new(),
        override_record: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability_entry::evaluate_capability;

    /// A synthetic (delanguaged) `MorphRuleOrder::Unordered` stratum with more loose rules than
    /// [`DEFAULT_ORDERING_MULTIPLICITY_BUDGET`] — this module's own "shaped synthetic grammar"
    /// gate: `preflight_findings` must raise a `ProvenBoundExceedsBudget`/`OrderingRuleCount`
    /// finding BEFORE any foma compile is attempted. Ported verbatim from `crate::capability`'s own
    /// identically-named test-only fixture generator (this crate's repo-wide "port a fixture across
    /// a module boundary" convention for a `pub(crate)`/private helper neither side can share
    /// directly).
    fn unordered_overflow_grammar_xml(rule_count: u32) -> String {
        let mut rules = String::new();
        let mut segs = String::new();
        for i in 0..rule_count {
            segs.push_str(&format!(
                r#"<SegmentDefinition id="cx{i}"><Representations><Representation>x{i}</Representation></Representations></SegmentDefinition>"#
            ));
            rules.push_str(&format!(
                r#"<MorphologicalRule id="mr{i}" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
                     <Name>r{i}</Name>
                     <MorphologicalSubrules>
                       <MorphologicalSubrule id="sub{i}">
                         <MorphologicalInput><PhoneticSequence id="stem{i}"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                         <MorphologicalOutput><InsertSegments><PhoneticShape>x{i}</PhoneticShape></InsertSegments><CopyFromInput index="stem{i}" /></MorphologicalOutput>
                       </MorphologicalSubrule>
                     </MorphologicalSubrules>
                     <MorphemeId>R{i}</MorphemeId>
                   </MorphologicalRule>"#
            ));
        }
        let rule_ids: Vec<String> = (0..rule_count).map(|i| format!("mr{i}")).collect();
        format!(
            r#"<HermitCrabInput><Language><Name>PreflightUnorderedFixture</Name>
              <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
              <CharacterDefinitionTable id="t1"><Name>Main</Name>
                <SegmentDefinitions>
                  <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
                  {segs}
                </SegmentDefinitions>
              </CharacterDefinitionTable>
              <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
              <Strata>
                <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="{ids}">
                  <Name>S</Name>
                  <MorphologicalRuleDefinitions>{rules}</MorphologicalRuleDefinitions>
                  <LexicalEntries>
                    <LexicalEntry id="eK" partOfSpeech="posV">
                      <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
                      <MorphemeId>K</MorphemeId>
                    </LexicalEntry>
                  </LexicalEntries>
                </Stratum>
              </Strata>
            </Language></HermitCrabInput>"#,
            ids = rule_ids.join(" "),
        )
    }

    #[test]
    fn preflight_raises_ordering_rule_count_finding_on_shaped_unordered_grammar() {
        let rule_count = DEFAULT_ORDERING_MULTIPLICITY_BUDGET as u32 + 1;
        let xml = unordered_overflow_grammar_xml(rule_count);
        let grammar = pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture load failed: {e}"));
        let findings = preflight_findings(&grammar);

        let finding = findings
            .iter()
            .find(|f| f.code == FindingCode::ProvenBoundExceedsBudget)
            .unwrap_or_else(|| {
                panic!("expected a ProvenBoundExceedsBudget preflight finding, got {findings:?}")
            });
        assert_eq!(finding.metric, Metric::OrderingRuleCount);
        assert_eq!(finding.severity, Severity::Critical);
        assert_eq!(finding.phase, Phase::Preflight);
        assert_eq!(finding.provenance, ValueProvenance::ProvenBound);
        assert_eq!(finding.value, MetricValue::Count(rule_count as u64));
        assert_eq!(
            finding.threshold,
            Some(MetricValue::Count(
                DEFAULT_ORDERING_MULTIPLICITY_BUDGET as u64
            ))
        );

        // This same grammar's ADR 0001 capability gate also resolves to Refuse (an unbounded
        // Unordered stratum is exactly `unordered-application.unbounded`, `capability.rs`'s own
        // `compose_envelope_refuses_unordered_morph_rule_order_grammar` test) -- the generic
        // semantic-uncertainty finding must ALSO be present, naming the same construct in a less
        // specific (grammar-wide) way.
        assert!(
            findings.iter().any(|f| f.severity == Severity::Critical
                && f.code == FindingCode::UnknownUnboundedConstruct),
            "expected the Refuse-derived semantic-uncertainty finding too, got {findings:?}"
        );
    }

    /// A comfortably-within-budget unordered stratum must raise no `OrderingRuleCount` finding —
    /// proving the check above is real gating, not an unconditional finding.
    #[test]
    fn preflight_raises_no_ordering_finding_when_within_budget() {
        let xml = unordered_overflow_grammar_xml(3);
        let grammar = pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture load failed: {e}"));
        let findings = preflight_findings(&grammar);
        assert!(
            !findings
                .iter()
                .any(|f| f.metric == Metric::OrderingRuleCount),
            "a within-budget unordered stratum must not raise an OrderingRuleCount finding: \
             {findings:?}"
        );
    }

    /// A grammar with no Refuse/ConfirmOnly-resolved construct, no unbounded quantifier, and a
    /// small rule-interaction product must raise no preflight finding at all — the empty-input
    /// convention every other health producer in this crate pins
    /// (`crate::health_evaluator::fst_health_evaluator_empty_report_is_ideal`). Same shape as
    /// `pg-cli`'s own `CLEAN_GRAMMAR_XML` (`pack.rs`'s test module), independently known to compose
    /// to `CompileDecision::Admit`.
    #[test]
    fn preflight_raises_nothing_for_a_clean_small_grammar() {
        const CLEAN_XML: &str = r#"<HermitCrabInput><Language><Name>PreflightCleanFixture</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>main</Name>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="e1-1"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>kat</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let grammar =
            pg_grammar::load(CLEAN_XML).unwrap_or_else(|e| panic!("fixture load failed: {e}"));
        assert_eq!(evaluate_capability(&grammar), CompileDecision::Admit);
        let findings = preflight_findings(&grammar);
        assert!(
            findings.is_empty(),
            "a clean, tiny grammar must raise no preflight finding: {findings:?}"
        );
    }

    /// Task 1.3's semantic-uncertainty scenario: an `Overwrite`-output `MprGroup` resolves to
    /// `CompileDecision::Refuse` (`capability.rs`'s own `compose_envelope_refuses_for_overwrite_
    /// group_alone` fixture, ported verbatim -- this crate's repo-wide "port a fixture across a
    /// module boundary" convention) — preflight must report it as a `Critical`
    /// `UnknownUnboundedConstruct` finding naming the Overwrite construct, before foma ever runs.
    /// Originally used a self-feeding (`multipleApplication="2"`) `Compounding` rule for this same
    /// purpose; `plan-construct-coverage-completion` task 4.1 promoted `compounding.recursive` to
    /// `ConfirmOnly` (`crate::capability::CompoundingRecursionSafePredicate`'s own doc), so that
    /// fixture no longer refuses and this test needed a different, still-permanently-refusing
    /// construct — `MprGroupOverwrite` (`MprGroupOverwriteFailClosedPredicate`, unconditional) is
    /// this crate's clearest such carve-out.
    #[test]
    fn preflight_raises_critical_finding_for_refuse_verdict() {
        const REFUSE_XML: &str = include_str!("../../../../conformance-staging/edge-cases/simultaneous-subrule-genuine-overlap/grammar.xml");
        let grammar =
            pg_grammar::load(REFUSE_XML).unwrap_or_else(|e| panic!("fixture load failed: {e}"));
        assert!(matches!(
            evaluate_capability(&grammar),
            CompileDecision::Refuse(_)
        ));
        let findings = preflight_findings(&grammar);

        let finding = findings
            .iter()
            .find(|f| f.severity == Severity::Critical)
            .unwrap_or_else(|| panic!("expected a Critical preflight finding, got {findings:?}"));
        assert_eq!(finding.code, FindingCode::UnknownUnboundedConstruct);
        assert_eq!(finding.phase, Phase::Preflight);
        assert_eq!(finding.provenance, ValueProvenance::Observed);
        assert!(
            finding
                .affected
                .iter()
                .any(|a| a.contains("prule 0 subrules 0/1")),
            "expected the simultaneous-overlap construct named: {finding:?}"
        );
    }

    /// Task 1.2's grammar-wide bounded-product case: enough morphological/phonological rules to
    /// cross [`RULE_PRODUCT_WARNING_THRESHOLD`] raises a `Predicted`/`Warning` finding naming the
    /// exact product. Exercised directly against [`rule_interaction_product_finding`] (a synthetic
    /// `CharacteristicsProfile` with only `cardinality` populated) rather than a large generated
    /// grammar, since this finding depends on nothing else in the profile.
    #[test]
    fn rule_interaction_product_finding_fires_above_threshold_not_below() {
        let mut above = CharacteristicsProfile::default();
        above.cardinality.mrule_count = 9;
        above.cardinality.prule_count = 9; // 81 > 64
        let finding = rule_interaction_product_finding(&above)
            .unwrap_or_else(|| panic!("expected a rule-interaction-product finding"));
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.provenance, ValueProvenance::Predicted);
        assert_eq!(finding.phase, Phase::Preflight);
        assert_eq!(finding.value, MetricValue::Count(81));
        assert!(finding.affected.is_empty());

        let mut below = CharacteristicsProfile::default();
        below.cardinality.mrule_count = 2;
        below.cardinality.prule_count = 2; // 4 <= 64
        assert!(rule_interaction_product_finding(&below).is_none());
    }
}
