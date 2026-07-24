//! Step 3 of `openspec/changes/reify-compilation-plans` (design.md **D4**, task 3.1): the
//! differential-correctness oracle -- ADR 0002's "the refactor pays for its own correctness check"
//! promise, made concrete. Per design.md's own framing, this is flagged as the change's genuinely
//! novel research contribution ("building >=2 independently-derived over-approximations of one
//! grammar and using their disagreement as a designed-in correctness oracle"); this module ships
//! ONLY D4's cheap, always-on tier.
//!
//! # What this module does NOT do (explicitly out of scope, per D4)
//! - **No confirm-engine integration.** D4's cheap tier as shipped elsewhere in this crate's own
//!   product (`crate::composite::FomaAnalyzer`) would run `confirm(propose_P1(w)) ==
//!   confirm(propose_P2(w))` through the trusted HC confirm engine. This module instead compares
//!   the two plans' raw `apply_up` result SETS directly -- [`build::build_controllable`]'s own
//!   `equivalence_tests` module's predicate, generalized to two arbitrary [`Plan`]s + a word list +
//!   shortest-witness reporting, per this task's own instruction. Wiring a real confirm pass in is
//!   future work, not this step's.
//! - **No exact-equivalence stretch tier.** D4's "expensive, opt-in tier" (decidable FST
//!   equivalence for finite-valued transducers) is explicitly marked a stretch goal, not a v1
//!   requirement, in design.md -- not attempted here.
//!
//! # The D1 soundness caveat this module must respect (task 1.4: now RESOLVED)
//! design.md D1's soundness invariant (added after Step 3a, `crate::build`'s own module doc) is:
//! **a node's compiled artifact must be a pure function of its `NodeId`** for any `NodeId`-keyed
//! memoization to be sound. That was **not true** in general for `Gate`/`Replace` pairing at Step
//! 3a -- [`build::build_controllable`] sidestepped it by being Gate-aware (re-deriving each group's
//! `subrule_ok` from the `Gate` node's own partition, never caching a compiled `Fsm` against a
//! shared `Replace` `NodeId`) rather than by a generic `NodeId`-memoizing interpreter. Task 1.4
//! closed the gap at its root: `crate::enumerate::enumerate_default` now builds one `Replace` node
//! PER GROUP, carrying that group's own `gated_subrules`/`group_key` directly in its
//! `crate::plan::ReplaceCascadeSpec` (that struct's own doc), so distinct groups get distinct
//! `Replace` `NodeId`s and `build_controllable` reads `subrule_ok` from the Replace node's own
//! content, not the `Gate` node's partition (`build`'s own module doc). This module calls
//! [`build::build_controllable`] itself for BOTH plans it diffs, so it inherits that same
//! now-content-pure behavior -- it never memoizes a compiled artifact by `NodeId` across the two
//! builds (still true, and now provably safe if it did). [`permute_gate_groups`] (below) is careful
//! to keep this sound too: it reorders a `Gate` node's `groups` and `children` IN LOCKSTEP (each
//! group's key travels with its own child, and -- since task 1.4 -- its own `Replace` node,
//! implicitly, as part of that child subtree), never separately -- so every group's `subrule_ok` is
//! still resolved from the correct key at `build_controllable` time, on both plans.
//!
//! # The oracle's comparison methodology
//! [`differential_oracle`] builds BOTH input plans via [`build::build_controllable`] (never
//! recomputing a partition/cascade itself -- same discipline as `build.rs`'s own module doc), then
//! for every word in the caller-supplied word list computes `apply_up`'s full result-string set on
//! each built net (an empty set, not a panic, for a plan whose build produced no net at all --
//! `GatedCompileResult::net`'s `None` case, e.g. every partition group empty). Words whose two
//! result sets are unequal are disagreements; among those, the SHORTEST disagreeing word (by `char`
//! count, ties broken lexicographically -- design.md D4's own words: "emit the shortest disagreeing
//! word") is reported, together with the symmetric difference of the two result sets (the
//! CFG-equivalence-tool pattern D4 cites). The selection logic itself
//! ([`resolve_verdict`]) is a small pure function over `(word, results_a, results_b)` triples,
//! deliberately factored out of the foma-build-heavy entry point so it can be unit-tested directly
//! against synthetic result sets (this module's own `shortest witness` tests) without needing a
//! grammar whose recognized surface forms happen to span several lengths.
//!
//! # The second topology: [`permute_gate_groups`]
//! A differential oracle needs two genuinely distinct [`Plan`]s that encode the SAME relation to be
//! a non-vacuous same-relation exercise. [`permute_gate_groups`] builds one: a copy of the input
//! plan with every `Gate` node's `partition.groups` (and paired `children`) reordered (reversed).
//! Because [`build::build_controllable`] folds every group's compiled network together with
//! [`crate::compose_budget::union_checked`] (commutative) and always finishes with
//! [`crate::compose_budget::minimize_checked`], a `Gate` node's group ORDER cannot affect the final
//! relation -- only membership does. Reordering therefore changes the `Gate` node's content address
//! ([`crate::plan::NodeId`] is `hash(kind, children, config)`, and both `partition.groups` and
//! `children` are part of that content) without changing what the built network recognizes: a real,
//! non-trivial differential-oracle pair, not two labels for the identical `Plan`.
//!
//! # Judgment call: `Result`, not a bare `OracleResult`
//! [`differential_oracle`] returns `Result<OracleResult, ComposeError>`, not a bare `OracleResult` --
//! [`build::build_controllable`] is itself fallible (a [`crate::compose_budget::ComposeBudget`] cap
//! can trip on either plan), and this module has no sound way to turn that failure into an
//! `OracleResult` variant (neither "the two plans agree" nor "the two plans disagree" is true when
//! one plan didn't build at all). Propagating `ComposeError` mirrors `build_controllable`'s own
//! `Result` convention rather than inventing a third `OracleResult` case for "didn't run".

use std::collections::{BTreeSet, HashSet};

use foma::apply::apply_init;
use foma::options::FomaOptions;
use foma::types::Fsm;

use pg_grammar::model::{Grammar, PhonRuleDef};

use crate::build::build_controllable;
use crate::compose_budget::{ComposeBudget, ComposeError};
use crate::plan::{GateGroupSpec, GatePartitionSpec, NodeId, Plan, PlanNodeKind};
use crate::replace::SegAlphabet;

/// The outcome of one [`differential_oracle`] run (design.md D4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OracleResult {
    /// Every word compared produced identical `apply_up` result sets on both plans.
    Agree,
    /// At least one word disagreed. `word` is the SHORTEST such word (D4's own words), ties broken
    /// lexicographically; `only_in_a`/`only_in_b` are the symmetric difference of the two plans'
    /// `apply_up` result sets for THAT word (D4: "the CFG-equivalence-tool pattern"); `plan_a_label`/
    /// `plan_b_label` echo [`differential_oracle`]'s own `labels` argument, so a caller printing this
    /// variant does not need to thread the labels through separately.
    Disagree {
        word: String,
        only_in_a: BTreeSet<String>,
        only_in_b: BTreeSet<String>,
        plan_a_label: String,
        plan_b_label: String,
    },
}

/// Every raw string `apply_up` yields for `word` against `net` (module doc: the same predicate
/// [`crate::build`]'s own `equivalence_tests` uses), encoded through `alphabet.encode_query` first.
/// `None` (no net at all -- [`crate::gate::GatedCompileResult::net`]'s `None` case) or a query that
/// fails to encode against this grammar's segment table both yield the EMPTY set, never a panic --
/// "this plan recognizes nothing" and "this word doesn't parse against this plan's net" are both
/// legitimate, comparable outcomes for a differential run, not error conditions.
fn apply_up_results(net: Option<&Fsm>, alphabet: &SegAlphabet, word: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(net) = net else {
        return out;
    };
    let Some(query) = alphabet.encode_query(word) else {
        return out;
    };
    let mut h = apply_init(net);
    for s in h.up(&query) {
        out.insert(s);
    }
    out
}

/// The pure selection core (module doc): given each word's two `apply_up` result sets, returns
/// `Agree` if all agree, else `Disagree` naming the SHORTEST disagreeing word (`char` count, ties
/// broken lexicographically by the word itself -- a total, deterministic order, per D4's own "break
/// ties deterministically" framing this task states explicitly). Factored out of
/// [`differential_oracle`] so it can be unit-tested directly against synthetic `(word, results_a,
/// results_b)` triples, independent of building any real `Fsm`.
fn resolve_verdict(
    per_word: Vec<(String, HashSet<String>, HashSet<String>)>,
    plan_a_label: &str,
    plan_b_label: &str,
) -> OracleResult {
    let mut disagreements: Vec<(String, BTreeSet<String>, BTreeSet<String>)> = per_word
        .into_iter()
        .filter(|(_, a, b)| a != b)
        .map(|(word, a, b)| {
            let only_in_a: BTreeSet<String> = a.difference(&b).cloned().collect();
            let only_in_b: BTreeSet<String> = b.difference(&a).cloned().collect();
            (word, only_in_a, only_in_b)
        })
        .collect();

    // Shortest-first, lexicographic tie-break: sort by (char count, word) and take the first --
    // deterministic regardless of `per_word`'s own input order.
    disagreements.sort_by(|(word_x, ..), (word_y, ..)| {
        (word_x.chars().count(), word_x).cmp(&(word_y.chars().count(), word_y))
    });

    match disagreements.into_iter().next() {
        None => OracleResult::Agree,
        Some((word, only_in_a, only_in_b)) => OracleResult::Disagree {
            word,
            only_in_a,
            only_in_b,
            plan_a_label: plan_a_label.to_string(),
            plan_b_label: plan_b_label.to_string(),
        },
    }
}

/// D4's cheap, always-on differential-correctness tier: builds BOTH `plan_a` and `plan_b` via
/// [`build_controllable`] (never recomputing a partition/cascade itself), then compares their
/// `apply_up` result sets over every word in `words`. Returns `Ok(OracleResult::Agree)` iff every
/// word's two result sets are identical; otherwise `Ok(OracleResult::Disagree { .. })` naming the
/// shortest disagreeing word (module doc, [`resolve_verdict`]). `labels` are `(plan_a`'s label,
/// `plan_b`'s label`)`, echoed back on a `Disagree` for a caller's own reporting -- purely
/// diagnostic, no bearing on the comparison itself.
///
/// `opts`/`g`/`alphabet`/`prules_in_order`/`budget` are threaded straight through to both
/// `build_controllable` calls -- the SAME grammar-derived inputs both plans must have been
/// enumerated against (mismatched inputs would make "the two plans encode the same relation" an
/// incoherent claim to begin with; this function does not attempt to detect that caller error, the
/// same trust convention `build_controllable` itself documents for its own `prules_in_order`
/// parameter).
///
/// # Errors
/// Propagates a [`ComposeError`] from either `build_controllable` call unchanged (module doc's
/// judgment-call note: no `OracleResult` variant means "one plan didn't build").
#[allow(clippy::too_many_arguments)] // mirrors build_controllable's own args, taken for BOTH plans
                                     // plus labels/words -- same convention as this crate's other
                                     // many-parameter entry points (replace.rs/preexpand.rs/emit.rs).
pub fn differential_oracle(
    plan_a: &Plan,
    plan_b: &Plan,
    labels: (&str, &str),
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    budget: &ComposeBudget,
    words: &[&str],
) -> Result<OracleResult, ComposeError> {
    let (label_a, label_b) = labels;

    let built_a = build_controllable(plan_a, opts, g, alphabet, prules_in_order, budget)?;
    let built_b = build_controllable(plan_b, opts, g, alphabet, prules_in_order, budget)?;

    let per_word: Vec<(String, HashSet<String>, HashSet<String>)> = words
        .iter()
        .map(|&word| {
            let a = apply_up_results(built_a.net.as_ref(), alphabet, word);
            let b = apply_up_results(built_b.net.as_ref(), alphabet, word);
            (word.to_string(), a, b)
        })
        .collect();

    Ok(resolve_verdict(per_word, label_a, label_b))
}

/// The shape of the callback [`copy_plan_transforming_gates`]/[`copy_node`] thread through their
/// recursive copy: given a `Gate` node's own `(partition, children)`, returns the `(partition,
/// children)` to install in the copy. Named purely to satisfy `clippy::type_complexity` -- not a
/// new abstraction beyond what those two functions' own doc comments already describe.
type GateTransform<'a> = dyn FnMut(&GatePartitionSpec, &[NodeId]) -> (GatePartitionSpec, Vec<NodeId>) + 'a;

/// Recursively rebuilds `plan` into a fresh [`Plan`] arena, applying `transform_gate` to every
/// `Gate` node's own `(partition, children)` pair as it is encountered -- BEFORE recursing into
/// whichever children `transform_gate` decides to keep, so a transform that drops a group's child
/// entirely never even copies that subtree into the new plan. Shared by [`permute_gate_groups`]
/// (this module's public second-topology generator) and this module's own test-only "drop a gate
/// group" deliberately-wrong-plan constructor, so the two graph-walks cannot silently drift apart.
///
/// Every non-`Gate` node is copied verbatim (same fragment/provenance/strategy/cascade, children
/// recursively copied) -- [`Plan::add_node`]'s own content-addressed dedup means an unaffected
/// subtree copied this way still interns to a stable `NodeId`, just possibly a different one than in
/// the source plan if any of ITS descendants changed (which, for the transforms this module ships,
/// never happens below a `Gate` node -- only the `Gate` node's own content changes).
fn copy_plan_transforming_gates(
    plan: &Plan,
    transform_gate: &mut GateTransform<'_>,
) -> Plan {
    let root = plan
        .root()
        .expect("copy_plan_transforming_gates requires a Plan with a root set");
    let mut new_plan = Plan::new();
    let new_root = copy_node(plan, root, &mut new_plan, transform_gate);
    new_plan.set_root(new_root);
    new_plan
}

fn copy_node(
    old_plan: &Plan,
    old_id: NodeId,
    new_plan: &mut Plan,
    transform_gate: &mut GateTransform<'_>,
) -> NodeId {
    match old_plan
        .get(old_id)
        .unwrap_or_else(|| panic!("dangling NodeId {old_id} while copying a Plan"))
    {
        PlanNodeKind::Leaf {
            fragment,
            provenance,
        } => new_plan.add_node(PlanNodeKind::Leaf {
            fragment: fragment.clone(),
            provenance: provenance.clone(),
        }),
        PlanNodeKind::Compose { children, strategy } => {
            let strategy = *strategy;
            let new_children: Vec<NodeId> = children
                .iter()
                .map(|&c| copy_node(old_plan, c, new_plan, transform_gate))
                .collect();
            new_plan.add_node(PlanNodeKind::Compose {
                children: new_children,
                strategy,
            })
        }
        PlanNodeKind::Union { children } => {
            let new_children: Vec<NodeId> = children
                .iter()
                .map(|&c| copy_node(old_plan, c, new_plan, transform_gate))
                .collect();
            new_plan.add_node(PlanNodeKind::Union {
                children: new_children,
            })
        }
        PlanNodeKind::Replace { cascade, children } => {
            let cascade = cascade.clone();
            let new_children: Vec<NodeId> = children
                .iter()
                .map(|&c| copy_node(old_plan, c, new_plan, transform_gate))
                .collect();
            new_plan.add_node(PlanNodeKind::Replace {
                cascade,
                children: new_children,
            })
        }
        PlanNodeKind::Gate {
            partition,
            children,
        } => {
            let (new_partition, kept_old_children) = transform_gate(partition, children);
            let new_children: Vec<NodeId> = kept_old_children
                .iter()
                .map(|&c| copy_node(old_plan, c, new_plan, transform_gate))
                .collect();
            new_plan.add_node(PlanNodeKind::Gate {
                partition: new_partition,
                children: new_children,
            })
        }
    }
}

/// The second same-relation topology this task asks for (module doc): a copy of `plan` with every
/// `Gate` node's `partition.groups` (and each group's OWN paired `children` entry) reversed. Each
/// group's key travels with its own child in lockstep -- the D1 soundness caveat this module doc
/// discusses: `build_controllable` re-derives a group's `subrule_ok` from THAT group's own key, so
/// as long as key and child stay paired, reordering groups cannot desync which key gates which
/// compiled network. Only the ORDER changes, never membership, so [`differential_oracle`] run over
/// `plan` and `permute_gate_groups(plan)` is expected to `Agree` (module doc: union is commutative,
/// the build always ends in `minimize_checked`).
///
/// # Panics
/// Via [`Plan::add_node`]'s own debug-only invariant, if `plan` contains a malformed `Gate` node
/// (groups/children length mismatch) -- not a new invariant this function introduces.
pub fn permute_gate_groups(plan: &Plan) -> Plan {
    copy_plan_transforming_gates(plan, &mut |partition, children| {
        assert_eq!(
            partition.groups.len(),
            children.len(),
            "permute_gate_groups: Gate node must have one child per partition group"
        );
        let mut paired: Vec<(GateGroupSpec, NodeId)> = partition
            .groups
            .iter()
            .cloned()
            .zip(children.iter().copied())
            .collect();
        paired.reverse();
        let groups: Vec<GateGroupSpec> = paired.iter().map(|(key, _)| key.clone()).collect();
        let kept_children: Vec<NodeId> = paired.iter().map(|(_, child)| *child).collect();
        (
            GatePartitionSpec {
                gated_subrules: partition.gated_subrules.clone(),
                groups,
            },
            kept_children,
        )
    })
}

#[cfg(test)]
mod tests {
    //! Three outcomes the task requires, in this order: (1) two genuinely distinct SAME-relation
    //! plans (`enumerate_default` vs. [`permute_gate_groups`] of it) -> `Agree`; (2) a deliberately
    //! WRONG second plan (one gate group dropped, module doc's `drop_last_gate_group`) -> a real
    //! `Disagree` naming a concrete word and a non-empty symmetric difference, proving the oracle is
    //! not vacuous; (3) the shortest-witness tie-break, tested directly against
    //! [`resolve_verdict`] with synthetic multi-length word data (this repo's tiny synthetic
    //! fixtures only ever recognize single-segment surface forms, so exercising the length tie-break
    //! through a real grammar+build would need a needlessly elaborate fixture -- testing the pure
    //! selection function directly is the more direct proof of this specific claim).

    use std::collections::HashSet;

    use pg_grammar::model::{Grammar, PhonRuleDef};

    use super::*;
    use crate::enumerate::enumerate_default;
    use crate::junctions::PhonologyProbe;

    fn load(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    fn prules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
        g.strata
            .iter()
            .flat_map(|s| &s.prules)
            .map(|&id| &g.prules[id.0 as usize])
            .collect()
    }

    /// One MPR-gated subrule and two entries realizing both truth values of that gate key -- the
    /// same shape as `enumerate.rs`'s own private `gated_two_group_fixture` / `build.rs`'s own
    /// duplicate of it (`gated_two_group_fixture_xml`), duplicated here again for the same reason
    /// those two modules each duplicate it rather than share it across a `#[cfg(test)]` boundary.
    /// Synthetic and delanguaged per this repo's own conformance-grammar convention: `e0` (no
    /// `ruleFeatures`) realizes gate key `[false]` (its underlying "p" surfaces unchanged); `e1`
    /// (`ruleFeatures="mpr1"`) realizes `[true]` (its underlying "p" surfaces as "q") -- so "p" and
    /// "q" are the two words that can only ever be produced by exactly one of the two gate groups,
    /// which is exactly the property both this module's Agree and Disagree tests need.
    fn oracle_gated_two_group_fixture_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>OracleGatedTwoGroupFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mpr1">f1</MorphologicalPhonologicalRuleFeature>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule1">
        <Name>gate1</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule requiredMPRFeatures="mpr1">
            <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prule1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e0" partOfSpeech="posV">
            <Allomorphs><Allomorph id="allo0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e0</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e1" partOfSpeech="posV" ruleFeatures="mpr1">
            <Allomorphs><Allomorph id="allo1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
    }

    /// Test-only deliberately-WRONG second topology (module doc, `copy_plan_transforming_gates`):
    /// drops the LAST gate group (post `enumerate_default`'s own sort-by-key, this is the group
    /// with the lexicographically-largest key -- `[true]` on the two-group fixture, i.e. entry
    /// `e1`, the ONLY entry that ever produces surface "q"). A real under-generating topology, not a
    /// no-op: [`differential_oracle`] run against the un-truncated plan must catch this.
    fn drop_last_gate_group(plan: &Plan) -> Plan {
        copy_plan_transforming_gates(plan, &mut |partition, children| {
            assert_eq!(partition.groups.len(), children.len());
            assert!(
                partition.groups.len() >= 2,
                "drop_last_gate_group needs >=2 groups to drop one and still have a non-empty Gate"
            );
            let keep = partition.groups.len() - 1;
            let groups = partition.groups[..keep].to_vec();
            let kept_children = children[..keep].to_vec();
            (
                GatePartitionSpec {
                    gated_subrules: partition.gated_subrules.clone(),
                    groups,
                },
                kept_children,
            )
        })
    }

    fn hs(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // -----------------------------------------------------------------------------------------
    // Outcome 1: two genuinely distinct, SAME-relation plans -> Agree.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn permuted_gate_groups_is_a_genuinely_different_plan() {
        let g = load(oracle_gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);

        let plan_a = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let plan_b = permute_gate_groups(&plan_a);

        assert_ne!(
            plan_a.root(),
            plan_b.root(),
            "permute_gate_groups must produce a plan with a different root NodeId (module doc: \
             group order is part of the Gate node's content address) -- otherwise this would not \
             be a real second topology for the oracle to diff"
        );
    }

    #[test]
    fn differential_oracle_agrees_on_permuted_gate_groups_of_the_same_grammar() {
        let g = load(oracle_gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();

        let plan_a = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let plan_b = permute_gate_groups(&plan_a);

        let result = differential_oracle(
            &plan_a,
            &plan_b,
            ("enumerate_default", "permute_gate_groups"),
            &opts,
            &g,
            &alphabet,
            &ro,
            &budget,
            &["p", "q"],
        )
        .expect("both plans must build successfully on this fixture");

        match result {
            OracleResult::Agree => {}
            OracleResult::Disagree {
                word,
                only_in_a,
                only_in_b,
                ..
            } => panic!(
                "two same-relation topologies (a grammar's default enumeration and its gate-group- \
                 permuted twin) must Agree, not Disagree -- got a real divergence at {word:?}: \
                 only_in_a={only_in_a:?}, only_in_b={only_in_b:?}. Per this task's own instruction, \
                 this must be reported as a genuine finding, never papered over."
            ),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Outcome 2: a deliberately WRONG second plan -> the oracle actually catches it.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn differential_oracle_catches_a_dropped_gate_group_as_a_real_disagreement() {
        let g = load(oracle_gated_two_group_fixture_xml());
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(&g);
        let phon = PhonologyProbe::new(&g);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();

        let plan_correct = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
        let plan_wrong = drop_last_gate_group(&plan_correct);
        assert_ne!(
            plan_correct.root(),
            plan_wrong.root(),
            "the truncated plan must be a genuinely different Plan"
        );

        let result = differential_oracle(
            &plan_correct,
            &plan_wrong,
            ("enumerate_default", "drop_last_gate_group (deliberately wrong)"),
            &opts,
            &g,
            &alphabet,
            &ro,
            &budget,
            &["p", "q"],
        )
        .expect("both plans must build successfully on this fixture (the truncated plan still has \
                  1 non-empty group left)");

        match result {
            OracleResult::Agree => panic!(
                "dropping an entire gate group (entry e1, the only entry that ever produces \
                 surface \"q\") changes the relation -- the oracle returning Agree here would mean \
                 it is VACUOUS, which is exactly what this test exists to rule out"
            ),
            OracleResult::Disagree {
                word,
                only_in_a,
                only_in_b,
                plan_a_label,
                plan_b_label,
            } => {
                assert_eq!(
                    word, "q",
                    "the dropped group is exactly what makes surface \"q\" analyzable -- \"q\" must \
                     be the (only) disagreeing word here"
                );
                assert!(
                    !only_in_a.is_empty() || !only_in_b.is_empty(),
                    "a real disagreement must carry a non-empty symmetric difference, got \
                     only_in_a={only_in_a:?} only_in_b={only_in_b:?}"
                );
                assert!(
                    only_in_b.is_empty(),
                    "the truncated (wrong) plan must UNDER-generate on \"q\" -- nothing should be \
                     unique to it; got only_in_b={only_in_b:?}"
                );
                assert_eq!(plan_a_label, "enumerate_default");
                assert_eq!(plan_b_label, "drop_last_gate_group (deliberately wrong)");
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Outcome 3: shortest-witness tie-break, tested directly against the pure selection core.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn resolve_verdict_reports_the_shortest_disagreeing_word_regardless_of_input_order() {
        let per_word = vec![
            // A LONGER disagreement, listed FIRST -- proves selection is by length, not by
            // first-encountered order.
            ("longerword".to_string(), hs(&["X"]), hs(&["Y"])),
            ("hi".to_string(), hs(&["A"]), hs(&["B"])),
            // An agreeing word, to prove agreements are simply excluded, not treated as a tie.
            ("z".to_string(), hs(&["same"]), hs(&["same"])),
        ];

        match resolve_verdict(per_word, "planA", "planB") {
            OracleResult::Disagree { word, .. } => {
                assert_eq!(word, "hi", "the SHORTEST disagreeing word must be reported")
            }
            OracleResult::Agree => panic!("expected a Disagree (two words genuinely differ)"),
        }
    }

    #[test]
    fn resolve_verdict_breaks_same_length_ties_lexicographically() {
        let per_word = vec![
            ("zz".to_string(), hs(&["X"]), hs(&["Y"])),
            ("aa".to_string(), hs(&["A"]), hs(&["B"])),
        ];

        match resolve_verdict(per_word, "planA", "planB") {
            OracleResult::Disagree { word, .. } => {
                assert_eq!(word, "aa", "same-length ties must break lexicographically")
            }
            OracleResult::Agree => panic!("expected a Disagree (two words genuinely differ)"),
        }
    }

    #[test]
    fn resolve_verdict_agrees_when_every_word_matches() {
        let per_word = vec![
            ("p".to_string(), hs(&["e0"]), hs(&["e0"])),
            ("q".to_string(), hs(&["e1"]), hs(&["e1"])),
        ];
        assert_eq!(
            resolve_verdict(per_word, "planA", "planB"),
            OracleResult::Agree
        );
    }
}
