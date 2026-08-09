//! The cheap, pre-compile health pass: reports constructs, quantifier/alternative products, alpha
//! tuples, templates/slots, predicted emitted work, peeled/confirm-only expansion, and
//! unknown/unbounded work without invoking foma. Unknown cost is not itself Critical when
//! construction is recall-preserving; any uncertainty that could omit an analysis fails closed.
//!
//! # Consume, never remeasure (same discipline as `crate::health_evaluator`)
//! `preflight_findings` takes a `&Grammar`, derives ONE
//! `crate::grammar_semantics::GrammarSemantics` from it, and reads exactly two existing, already-
//! tested, pure-Rust (no foma, no I/O) facts off it — never re-derives their logic itself:
//! - `crate::grammar_semantics::GrammarSemantics::characteristics` — this crate's own one-time,
//!   exhaustive walk over every represented `pg_grammar::model::Grammar` construct variant. Its
//!   `crate::capability::CharacteristicsProfile::cardinality`
//!   (`crate::capability::GrammarCardinality`) and per-observation detail structs
//!   (`crate::capability::QuantifierPatternDetail::all_bounded`,
//!   `crate::capability::UnorderedStratumDetail::rule_count`/`within_bound`) feed this module's
//!   cardinality/bounded-product findings verbatim — never re-walked from the grammar.
//! - `crate::capability_entry::best_case_across_backends` — the SAME capability-gate
//!   entry point `pg-cli`'s own `run_capability_gate`/`pangloss pack` already call (through its
//!   `&Grammar` front end `evaluate_capability`), composing `characterize` with the predicate
//!   registry (`crate::capability::compose_envelope`/`crate::capability::default_registry`) into
//!   one final `crate::capability::CompileDecision`. This module reuses that FINAL,
//!   predicate-resolved verdict directly rather than re-implementing the disposition/predicate-
//!   resolution logic itself — a raw, unresolved `crate::capability::Disposition::ConfigPredicate`
//!   characteristic (e.g. `Compounding`, `UnorderedMorphRuleApplication`, `QuantifierPattern`) only
//!   resolves to a real `ConfirmOnly`-vs-`Refuse` verdict THROUGH that predicate registry, so
//!   reasoning from raw per-kind dispositions alone (without running the registry) would
//!   misclassify most `ConfigPredicate` characteristics. Both this module and `evaluate_capability`
//!   read the SAME memoized profile off one `crate::grammar_semantics::GrammarSemantics`, so a
//!   `pangloss fst-health` run characterizes once rather than running `characterize`'s real
//!   `foma::types::Fsm` construction for `Simultaneous`-mode subrules twice.
//!
//! # Two distinct axes this module keeps separate
//! - **Semantic uncertainty** (`semantic_uncertainty_finding`): [`crate::capability::CompileDecision::
//!   Refuse`] — at least one construct has no predicate-proven recall-preserving compilation path at
//!   all. This preflight walk cannot guarantee every HermitCrab analysis survives, so it reports a
//!   `Critical` finding naming every `crate::capability::CapabilityDiagnostic` the gate collected.
//!   This finding never itself blocks the actual compiler pass (it is evidence, not a second gate —
//!   `pg-cli`'s own `run_capability_gate`/`pangloss pack` are the real enforcement points a caller
//!   consults separately); it is this report's own honest record of the same fact, consistent with
//!   (never duplicating the logic of) that gate.
//! - **Cost uncertainty** (`cost_uncertainty_finding`, `unbounded_quantifier_findings`):
//!   `crate::capability::CompileDecision::ConfirmOnly` (a first-class, non-failure verdict:
//!   propose the superset, HermitCrab confirm prunes false positives) or a specific
//!   `crate::capability::QuantifierPatternDetail::all_bounded` occurrence marked `false`. Always
//!   `Warning`, never `Critical` on its own (unknown cost is not itself Critical when construction
//!   is recall-preserving) — an actual budget trip during the real compile is a completely
//!   different, already-handled code path: `crate::health_evaluator::compose_error_finding`'s
//!   `crate::health::FindingCode::ResourceBudgetReached`/
//!   `crate::health::FindingCode::ProvenBoundExceedsBudget` arms, which this module's own
//!   findings never construct.
//!
//! # Bounded products
//! `unordered_stratum_findings` reuses [`crate::capability::CharacteristicsProfile::
//! unordered_stratum_details`]'s ALREADY-COMPUTED `rule_count`/`within_bound` against the SAME
//! `crate::compose_budget::DEFAULT_ORDERING_MULTIPLICITY_BUDGET` the real compile-time check
//! (`crate::unordered::check_unordered_strata_bound`, surfaced post-hoc by
//! `crate::health_evaluator::compose_error_finding`'s `OrderingMultiplicityExceeded` arm) trips
//! against — the exact count is already known before foma ever runs, so this finding's
//! `crate::health::ValueProvenance` is `ProvenBound`, not a heuristic guess, and its severity is
//! `Critical` (an exact count proven to exceed budget can reject work before allocation). This
//! deliberately duplicates part of what `semantic_uncertainty_finding`'s `Refuse` case already
//! names in a less specific way (a grammar-wide `Refuse` diagnostic list) with a MORE specific,
//! metric-tagged finding for this one construct — both are kept, since
//! `crate::health::Metric::OrderingRuleCount` carries information (the exact rule count vs. the
//! budget) the generic diagnostic-list finding does not.
//!
//! `rule_interaction_product_finding` computes bounded products for alternatives, templates, and
//! slots: `crate::capability::GrammarCardinality::mrule_count` times `prule_count` is a cheap,
//! generic proxy for how much morphological x phonological rule-interaction surface a grammar
//! presents. `RULE_PRODUCT_WARNING_THRESHOLD` is a conservative, provisional placeholder (this
//! crate's own repeated convention — mirrors
//! `crate::health_evaluator::APPROACHING_BUDGET_WARNING_FRACTION`'s identical disclaimer): no
//! real-grammar calibration evidence exists yet for this specific product, so this finding is
//! `Predicted`/`Warning` only, never something that can reject a compile on its own.
//!
//! # Design notes
//! 1. **Every uncertainty finding reuses `crate::health::FindingCode::UnknownUnboundedConstruct`**,
//!    at different severities (`Critical` for `Refuse`, `Warning` for `ConfirmOnly`/unbounded
//!    quantifiers/the rule-interaction product) — the SAME "same code, severity carries the
//!    distinction" pattern `crate::health_evaluator`'s own `unsupported_tier_finding`/
//!    `partial_tier_finding` already established for `FomaTier::Unsupported` vs. `FomaTier::Partial`,
//!    deliberately diverging from that code's general "not itself Critical" framing. No new
//!    `FindingCode` is minted for this.
//! 2. **`semantic_uncertainty_finding`'s `affected` names each [`crate::capability::
//!    CapabilityDiagnostic::construct`] string verbatim** (the same field `pg-cli`'s own
//!    `run_capability_gate`/`pangloss pack` already print to stderr) — never a re-derived
//!    identifier scheme.
//! 3. **`rule_interaction_product_finding`'s `affected` is empty** — this is a grammar-wide
//!    cardinality fact, not about any one construct, matching `crate::health_evaluator`'s own
//!    "grammar-level findings with no specific construct identifier ... leave `affected` empty"
//!    convention (`payload_size_finding`).

use pg_grammar::model::Grammar;

use crate::capability::{CharacteristicsProfile, CompileDecision, ObservationDetail};
use crate::capability_entry::best_case_across_backends;
use crate::compose_budget::DEFAULT_ORDERING_MULTIPLICITY_BUDGET;
use crate::grammar_semantics::GrammarSemantics;
use crate::health::{
    FindingCode, HealthFinding, Metric, MetricValue, Phase, Severity, ValueProvenance,
};

/// A conservative, uncalibrated placeholder (see this module's doc, "Bounded products"); never used to reject a compile, `Predicted`/`Warning` evidence only.
const RULE_PRODUCT_WARNING_THRESHOLD: u64 = 64;

/// The preflight walker: every `crate::health::HealthFinding` this crate can derive BEFORE any
/// foma compile is attempted, from `g` alone. See this module's own doc for the
/// semantic-vs-cost-uncertainty split and the bounded-product findings.
pub fn preflight_findings(g: &Grammar) -> Vec<HealthFinding> {
    preflight_findings_with_semantics(&GrammarSemantics::derive(g))
}

/// `preflight_findings` over an already-derived `GrammarSemantics` -- so a caller running
/// preflight alongside its own capability gate (`pangloss fst-health` is exactly such a caller)
/// characterizes once in total, not once per call site.
pub fn preflight_findings_with_semantics(semantics: &GrammarSemantics<'_>) -> Vec<HealthFinding> {
    // One derivation, shared, rather than a second characterize walk inside the join.
    let profile = semantics.characteristics();
    let decision = best_case_across_backends(semantics);

    let mut findings = Vec::new();
    findings.extend(semantic_uncertainty_finding(&decision));
    findings.extend(cost_uncertainty_finding(&decision));
    findings.extend(unbounded_quantifier_findings(profile));
    findings.extend(unordered_stratum_findings(profile));
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

/// `CompileDecision::ConfirmOnly`: recall-preserving, but this preflight stage has no proven cost bound for the construct(s) that landed here. Always `Warning`/`Predicted`; `None` for `Admit`/`Refuse`.
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

/// One `Predicted`/`Warning` finding per rule with a genuinely unbounded (`max="-1"`) quantifier occurrence.
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

/// Bounded-product case for `MorphRuleOrder::Unordered` strata: `within_bound == false` means the exact rule count is already proven to exceed `DEFAULT_ORDERING_MULTIPLICITY_BUDGET`, so this is `ProvenBound`/`Critical`.
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

/// The grammar-wide bounded-product case (module doc, "Bounded products"); `None` at or below `RULE_PRODUCT_WARNING_THRESHOLD`.
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
    use crate::capability_entry::best_case_across_backends;

    /// A synthetic `Unordered` stratum with more loose rules than `DEFAULT_ORDERING_MULTIPLICITY_BUDGET`, to check `preflight_findings` raises `ProvenBoundExceedsBudget`/`OrderingRuleCount` before any foma compile.
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

        // The ordering finding rests on the profile's own rule count, so it survives the JOIN no longer refusing.
        assert_eq!(
            best_case_across_backends(&GrammarSemantics::derive(&grammar)),
            CompileDecision::ConfirmOnly,
            "one compiler can still handle this grammar, so the join must not refuse it"
        );
        assert!(
            !findings.iter().any(|f| f.severity == Severity::Critical
                && f.code == FindingCode::UnknownUnboundedConstruct),
            "the semantic-uncertainty finding is Refuse-derived, so it must be absent here: \
             {findings:?}"
        );
    }

    /// A comfortably-within-budget unordered stratum must raise no `OrderingRuleCount` finding, proving the check above is real gating.
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

    /// A clean grammar (no Refuse/ConfirmOnly construct, no unbounded quantifier, small rule-interaction product) must raise no preflight finding at all.
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
        assert_eq!(
            best_case_across_backends(&GrammarSemantics::derive(&grammar)),
            CompileDecision::Admit
        );
        let findings = preflight_findings(&grammar);
        assert!(
            findings.is_empty(),
            "a clean, tiny grammar must raise no preflight finding: {findings:?}"
        );
    }

    /// A `Refuse` verdict must produce a `Critical` finding naming the construct; the fixture reduplicates on a `RealizationalRule` because only a construct EVERY compiler declines reaches the JOIN.
    /// See docs/research/pg-foma-capability-design-notes.md.
    #[test]
    fn preflight_raises_critical_finding_for_refuse_verdict() {
        const REFUSE_XML: &str = r#"<HermitCrabInput><Language><Name>RedupRealizational</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRules="rrRedupBad">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <RealizationalRule id="rrRedupBad">
                  <Name>redupBad</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="subRedupBad">
                      <MorphologicalInput>
                        <PhoneticSequence id="qA"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </MorphologicalInput>
                      <MorphologicalOutput redupMorphType="suffix">
                        <CopyFromInput index="qA" />
                        <CopyFromInput index="qA" />
                      </MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                  <MorphemeId>REDBAD</MorphemeId>
                </RealizationalRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let grammar =
            pg_grammar::load(REFUSE_XML).unwrap_or_else(|e| panic!("fixture load failed: {e}"));
        assert!(matches!(
            best_case_across_backends(&GrammarSemantics::derive(&grammar)),
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
                .any(|a| a.contains("mrule 0 allomorph #0")),
            "expected the non-peel-eligible reduplication construct named: {finding:?}"
        );
    }

    /// Crossing `RULE_PRODUCT_WARNING_THRESHOLD` raises a `Predicted`/`Warning` finding naming the exact product; exercised directly against a synthetic `CharacteristicsProfile` since this finding depends on nothing else in the profile.
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
