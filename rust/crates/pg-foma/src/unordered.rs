//! The compile-time cardinality gate for
//! `MorphRuleOrder::Unordered` (`pg-grammar/src/model.rs:1057-1060`; `StratumDef.mrule_order`,
//! `1067`) -- the second real production consumer of
//! `crate::compose_budget::ComposeBudget`'s chain-depth-shaped budget discipline, after
//! `crate::peel::ReduplicationPeeler`'s own per-word chain-depth check
//! (`crate::compose_budget`'s "Chain-depth dimension" section doc).
//!
//! # What `Unordered` actually reaches, and what this module gates
//! `pg-rules/src/cascade.rs` ports three cascades off one shared recursion shape: `Cascade::linear`
//! (phonological rules), `Cascade::permutation` (a `Linear` stratum: non-decreasing rule-index
//! recursion), and `Cascade::combination` (an `Unordered` stratum: the module's own "k!-walk over
//! rule subsets", `cascade.rs:8-9`). `pg-rules/src/stratum.rs::run_mrule_cascade`/
//! `synth_apply_mrules` dispatch a stratum's OWN `mrule_order` to `permutation`/`combination`
//! respectively -- `Unordered`'s reachable derivation set is a strict superset of what the
//! identical rule set would reach under `Linear` (never the reverse), so a faithful *propose*-side
//! union must be at least as permissive.
//!
//! # The ordering-union proposal IS an existing mechanism, not a new one
//! `crate::emit::build_deriv_chain` (the standalone/loose, non-template derivation-layer lexc
//! builder every stratum's `Role::Prefix`/`Role::Suffix`/`Role::None` loose mrules already compile
//! through, REGARDLESS of `mrule_order`) offers **every** candidate rule at **every** one of its
//! `depth = rules.len()` chain levels, unconditional on any `required_syn_fs`/prior-rule state --
//! it recurses in any order over any subset of the stratum's rules, mirroring
//! `combination_rec`'s own loop shape rather than `permutation_rec`'s. This is, on inspection,
//! EXACTLY this pre-existing construction: it was never restricted to a
//! non-decreasing rule-index walk the way `permutation_rec` is (no code in `crate::emit` branches
//! on `MorphRuleOrder` at all -- confirmed by grep: zero references outside this module,
//! `crate::capability`, `crate::morphotactics`'s doc-only mention, and `crate::peel`'s doc-only
//! citation). Being unconditional at propose time (confirm alone enforces `required_syn_fs`/
//! rule-order legality, via the REAL `pg_rules::stratum::run_mrule_cascade`), it is trivially a
//! superset of ANY cascade semantics' reachable set, `Unordered`'s included -- this is proven by
//! oracle containment in `tests/cover_unordered_morph_rules.rs`, not merely asserted here.
//!
//! **The load-bearing finding this module depends on: a separate proof establishes that the
//! resulting composed proposal's language equals the union over every admissible ordering's
//! surface output.** It is deliberately NOT
//! the same claim as `morphotactics.rs`'s own "Linear-as-Unordered" pruning convention (the
//! existing morphotactic-legality over-approximation is not itself treated as a
//! proposal-language proof): that automaton characterizes chain-ATTACHMENT legality for the
//! PHONOLOGICAL fusion/interdigitation composite builders (`crate::preexpand::extend`/
//! `crate::emit::struct_extend`), a DIFFERENT code path from `build_deriv_chain`'s ordinary
//! derivation layers -- a grammar with zero phonological rules and zero `Role::Infix` rules never
//! even builds that automaton's consuming callers at all (`crate::preexpand::should_run` is
//! `false`), yet still gets full `Unordered` containment purely through `build_deriv_chain`. The
//! synthetic fixture's `no_phonology_no_infix_rules_isolates_build_deriv_chain` test is the witness
//! that isolates this.
//!
//! # What THIS module adds: the chain-depth-bounded / unbounded split's compile-time gate
//! `build_deriv_chain`'s `depth` for a role zone equals that zone's rule count (its own doc:
//! `depth = rules.len().max(DERIV_DEPTH_MIN)`) -- so an `Unordered` stratum's own loose-rule count
//! is EXACTLY the quantity whose growth predicts this construction's compiled-network cost (each
//! extra level offers every rule again: `O(rule_count)` extra levels x `O(rule_count)` per-level
//! arcs). `check_unordered_strata_bound` is the compile-time gate `crate::analyzer::
//! FomaProposer::new_with_budget` calls, once per grammar, BEFORE handing any lexc source to the
//! foma compiler: `unordered-application.chain-depth-bounded` (within
//! `crate::compose_budget::ComposeBudget::ordering_multiplicity_cap`) proceeds unchanged;
//! `unordered-application.unbounded` (exceeding it) is refused with a typed
//! `crate::compose_budget::ComposeError::OrderingMultiplicityExceeded` -- an honest, deterministic
//! refusal, never a silent truncation and never an attempt to actually build the (potentially very
//! large, though NOT exponential by construction -- see this module's doc above) network.
//!
//! # Big-O
//! `check_unordered_strata_bound` itself is `O(strata + total mrules)` -- one pass over
//! `g.strata`/`sd.mrules`, no FST construction, no recursion. The construction it gates
//! (`build_deriv_chain`, for a zone with `n` `Unordered`-stratum-contributed loose rules) is
//! `O(n^2)` states/arcs (n levels x n rules/level) -- polynomial, NOT the `O(n!)` "k!-walk" the
//! confirm-side combination cascade must itself explore (`cascade.rs`'s own naming) -- because
//! `build_deriv_chain` encodes the union of admissible orderings IMPLICITLY, as a shared per-level
//! choice point, rather than enumerating each of the (up to) `n!` orderings as a separate literal
//! path. This is a deliberate choice: the calibrated bound gates
//! `rule_count` directly (a sound, conservative proxy for the combinatorial
//! danger) rather than a literally-computed ordering count, since no code path in this crate
//! actually materializes `n!` distinct candidates for this construct.
//!
//! # Runtime-feature declaration
//! **None required.** `build_deriv_chain`'s construction fully LOWERS into the compiled FST network
//! at compile time (most constructs are fully lowered and impose no runtime
//! requirement) -- there is no query-time operation analogous to
//! `crate::peel::RUNTIME_FEATURE_REDUPLICATION_PEEL`'s per-word peel op. Confirmed before
//! declaring anything, rather than inventing a placeholder
//! constant with nothing to name.

use pg_grammar::model::{Grammar, MorphRuleOrder, StratumId};

use crate::compose_budget::{ComposeBudget, ComposeError};

/// `crate::compose_budget::ComposeError`'s `site` label for every check this module makes.
const ORDERING_MULTIPLICITY_SITE: &str = "unordered::check_unordered_strata_bound";

/// One `Unordered` stratum's own cardinality facts (the chain-depth-bounded cardinality
/// check) -- shared by `check_unordered_strata_bound` (the compile-time gate) and
/// `crate::capability`'s `crate::capability::UnorderedStratumDetail` (the STATIC characterization,
/// computed independently over the same `rule_count`/`within_bound` facts so the declared capability
/// verdict and the real compile-time refusal can never silently drift apart).
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnorderedStratumMetrics {
    pub stratum: StratumId,
    pub rule_count: usize,
    pub within_bound: bool,
}

/// One stratum's own `UnorderedStratumMetrics`, computed against
/// `crate::compose_budget::DEFAULT_ORDERING_MULTIPLICITY_BUDGET` directly (not a live
/// `ComposeBudget` instance) -- `crate::capability::characterize` is a pure, `Grammar`-only
/// static projection with no `ComposeBudget` in scope anywhere else in that module,
/// so `crate::capability::UnorderedStratumDetail` is built directly from THIS function's own
/// result (not a re-derived copy of its formula), sharing the SAME calibrated constant
/// `check_unordered_strata_bound`'s own production-default `ComposeBudget::from_env`
/// configures. Called for EVERY stratum by `crate::capability::characterize`'s own per-stratum
/// walk, not just `Unordered` ones -- callers that only want `Unordered` strata should filter on
/// `unordered_stratum_metrics` instead.
pub(crate) fn stratum_metrics(
    stratum: StratumId,
    sd: &pg_grammar::model::StratumDef,
) -> UnorderedStratumMetrics {
    let rule_count = sd.mrules.len();
    UnorderedStratumMetrics {
        stratum,
        rule_count,
        within_bound: rule_count <= crate::compose_budget::DEFAULT_ORDERING_MULTIPLICITY_BUDGET,
    }
}

/// Every `Unordered` stratum's own `UnorderedStratumMetrics` (`stratum_metrics`, filtered to
/// `MorphRuleOrder::Unordered` strata only) -- this module's own tests use this directly; production
/// code goes through `check_unordered_strata_bound`. `#[allow(dead_code)]`: only this module's
/// `#[cfg(test)]` tests call it (a plain `--lib` build never does), mirroring
/// `crate::compose_budget::ComposeBudget::chain_depth_cap`'s own convention for a test-only reader.
#[allow(dead_code)]
pub(crate) fn unordered_stratum_metrics(g: &Grammar) -> Vec<UnorderedStratumMetrics> {
    g.strata
        .iter()
        .enumerate()
        .filter(|(_, sd)| sd.mrule_order == MorphRuleOrder::Unordered)
        .map(|(i, sd)| stratum_metrics(StratumId(i as u8), sd))
        .collect()
}

/// The compile-time gate (module doc): `Err` iff SOME `Unordered` stratum's own loose-rule count
/// exceeds `budget`'s `ComposeBudget::ordering_multiplicity_cap` -- checked against every
/// `Unordered` stratum independently (two strata receive independent capability
/// verdicts, and neither is inferred from the other), returning the FIRST breach found
/// (document stratum order) rather than collecting every one -- mirrors
/// `crate::morphotactics::EnumerationBudget`'s own "fail fast, do not keep computing once tripped"
/// discipline; a caller that wants every breach can call `unordered_stratum_metrics` directly.
pub(crate) fn check_unordered_strata_bound(
    g: &Grammar,
    budget: &ComposeBudget,
) -> Result<(), ComposeError> {
    for sd in &g.strata {
        if sd.mrule_order == MorphRuleOrder::Unordered {
            budget.check_ordering_multiplicity(sd.mrules.len(), ORDERING_MULTIPLICITY_SITE)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic fixture generator: one stratum declaring `order` with `rule_count` trivial suffix rules; only order and rule count matter to this module's checks, so every rule is otherwise as bare as the loader accepts.
    fn stratum_xml(order: &str, rule_count: u32) -> String {
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
        let rules_attr = if rule_ids.is_empty() {
            String::new()
        } else {
            format!(r#" morphologicalRules="{}""#, rule_ids.join(" "))
        };
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>UnorderedBoundFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        {segs}
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="{order}"{rules_attr}>
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>{rules}</MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#,
        )
    }

    fn load(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    #[test]
    fn linear_stratum_is_never_reported_regardless_of_rule_count() {
        let g = load(&stratum_xml("linear", 10));
        assert!(
            unordered_stratum_metrics(&g).is_empty(),
            "a Linear stratum must never appear in Unordered-only metrics"
        );
        let budget = ComposeBudget::unbounded().with_ordering_multiplicity_cap(6);
        assert!(
            check_unordered_strata_bound(&g, &budget).is_ok(),
            "a Linear stratum's rule count must never trip the Unordered-only budget"
        );
    }

    #[test]
    fn unordered_stratum_within_bound_reports_within_bound_and_passes_the_gate() {
        let g = load(&stratum_xml("unordered", 3));
        let metrics = unordered_stratum_metrics(&g);
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].rule_count, 3);
        assert!(metrics[0].within_bound);

        let budget = ComposeBudget::unbounded().with_ordering_multiplicity_cap(6);
        assert!(check_unordered_strata_bound(&g, &budget).is_ok());
    }

    /// Proves the gate genuinely reads the budget it is handed, not a hardcoded threshold: a 7-rule stratum is well within the production default but still trips a deliberately small explicit cap of 6.
    #[test]
    fn check_unordered_strata_bound_trips_on_an_explicit_low_cap() {
        let g = load(&stratum_xml("unordered", 7));
        let metrics = unordered_stratum_metrics(&g);
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].rule_count, 7);
        assert!(
            metrics[0].within_bound,
            "7 rules must be within the production default (100)"
        );

        let budget = ComposeBudget::unbounded().with_ordering_multiplicity_cap(6);
        let err = check_unordered_strata_bound(&g, &budget)
            .expect_err("a 7-rule Unordered stratum must trip an EXPLICIT cap of 6");
        match err {
            ComposeError::OrderingMultiplicityExceeded {
                rule_count, limit, ..
            } => {
                assert_eq!(rule_count, 7);
                assert_eq!(limit, 6);
            }
            other => panic!("expected OrderingMultiplicityExceeded, got {other:?}"),
        }
    }

    #[test]
    fn zero_rule_unordered_stratum_is_trivially_within_bound() {
        let g = load(&stratum_xml("unordered", 0));
        let metrics = unordered_stratum_metrics(&g);
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].rule_count, 0);
        assert!(metrics[0].within_bound);
    }

    #[test]
    fn production_default_within_bound_matches_the_calibrated_constant() {
        // Must agree with the calibrated constant exactly at the boundary: the default itself is within bound, one past it is not.
        let at_default = load(&stratum_xml(
            "unordered",
            crate::compose_budget::DEFAULT_ORDERING_MULTIPLICITY_BUDGET as u32,
        ));
        assert!(unordered_stratum_metrics(&at_default)[0].within_bound);

        let one_past = load(&stratum_xml(
            "unordered",
            crate::compose_budget::DEFAULT_ORDERING_MULTIPLICITY_BUDGET as u32 + 1,
        ));
        assert!(!unordered_stratum_metrics(&one_past)[0].within_bound);
    }

    #[test]
    fn production_default_budget_refuses_a_deep_unordered_stratum() {
        // Uses the production construction path (`FomaProposer::new`, `ComposeBudget::from_env`, no explicit cap) so the assertion below is about the calibrated default rather than a hand-set cap.
        let g = load(&stratum_xml(
            "unordered",
            crate::compose_budget::DEFAULT_ORDERING_MULTIPLICITY_BUDGET as u32 + 1,
        ));
        let budget = ComposeBudget::from_env();
        let err = check_unordered_strata_bound(&g, &budget)
            .expect_err("a stratum one rule past the calibrated default must refuse");
        assert!(matches!(
            err,
            ComposeError::OrderingMultiplicityExceeded { .. }
        ));
    }
}
