//! Stage 3 of `openspec/changes/add-pairwise-grammar-interaction-coverage` (the REFRAMED design —
//! see that change's `design.md`/`proposal.md`/`specs/grammar-interactions/spec.md`, and
//! `docs/adr/0001-honest-capability-boundary.md`): tree-structured node/subtree interaction
//! coverage over the REIFIED COMPILATION PLAN (`crate::plan`/`crate::enumerate::enumerate_default`),
//! not pairwise covering arrays over raw grammar "knobs" — the design's own reframe, because the
//! plan DAG's composition nodes are exactly where constructs meet and emergent hazards
//! (feeding/bleeding, order-dependence) actually arise.
//!
//! **BUILD-BREAKING as of 2026-07-26** (`openspec/changes/plan-construct-coverage-completion`
//! tasks.md 6.3, design.md §D7 step 7 — "Flip both cross-checks ... This is the finish line, not a
//! follow-on cleanup step"): this module's own integration test (`tests/
//! plan_interaction_coverage_gate.rs`) now asserts `uncovered().is_empty()` over the full
//! discovered corpus, not merely that the report runs and is non-empty. The flip followed the same
//! discipline `conformance_coverage_gate.rs`'s own flip did (that module's own doc; ADR 0001's
//! "'Supported' is mechanically gated..." precedent; a green build-breaking gate that can silently
//! start lying is worse than an advisory report, because the green light is what gets cited):
//! **zero `Uncovered` required tuples** confirmed against the real corpus; non-vacuity re-checked
//! (the 7-shape closed set, `unexpected_tuples.is_empty()`, and a non-empty discovered-fixture
//! corpus are all still independently asserted); and — unlike the sibling flip
//! (`docs/conformance/shared-construct-id-analysis.md`) — **no analogous "shared coarser id"
//! inheritance risk exists here**: every [`AdjacencyTuple`] is already this module's own
//! finest-grained unit (there is no coarser sibling tuple a finer one could borrow evidence from),
//! and [`observed_adjacency_tuples`]/[`compute_interaction_coverage`] only ever credit a tuple from
//! an actual parent-child edge present in a caller-supplied, per-fixture reified [`Plan`] — a tuple
//! cannot be marked `Covered` by a fixture that merely contains both node kinds somewhere without
//! the specific edge between them. See "The coverage report" section below for the fuller argument.
//! Nothing in `plan.rs`/`enumerate.rs`/`build.rs`/`oracle.rs`/`capability.rs` is modified by this
//! module — read/reuse only.
//!
//! # The tuple model (deliverable 1)
//! An [`AdjacencyTuple`] is a `(parent PlanNodeKind kind_name, child kind_name, child's own Leaf-
//! fragment-kind detail if the child is a [`crate::plan::PlanNodeKind::Leaf`], the [`crate::plan::
//! ComposeStrategy`] name if either endpoint is a [`crate::plan::PlanNodeKind::Compose`])` — the
//! design's own "(parent kind, child kind) pairs, and where cheap (parent, child, ComposeStrategy)
//! triples" (task's own deliverable 1 wording). [`legal_adjacency_tuples`] is the CLOSED set of
//! seven shapes [`crate::enumerate::enumerate_default`] — this crate's only enumerator strategy
//! today — can ever produce, read directly off that module's own "Shape" doc diagram:
//! ```text
//! (Union,   Gate)
//! (Union,   Leaf/CompositeEmissionMarker)
//! (Union,   Leaf/StructuralCompositeMarker)
//! (Gate,    Compose[Static])
//! (Compose, Leaf/LexiconFragment)[Static]
//! (Compose, Replace)[Static]
//! (Replace, Leaf/RewriteRule)
//! ```
//! A SECOND enumerator strategy (none exists yet) would need this list extended — a documented scope
//! boundary, not an oversight: this module characterizes ONE enumerator's plan shape, per this
//! step's own "real, working, advisory-first slice" instruction, not a general cross-product over
//! every [`crate::plan::PlanNodeKind`] pairing (most of which — e.g. `Leaf -> Leaf` — are not
//! anything `enumerate_default` (or, structurally, any sane enumerator) could ever produce, since
//! [`crate::plan::PlanNodeKind::Leaf`] never has children at all).
//!
//! Each tuple is TAGGED with the [`crate::capability::CharacteristicKind`]s its endpoints carry
//! (deliverable 1's "each tagged with the characteristics/constructs those nodes carry (via the Leaf
//! provenance + the profile)"): a [`crate::plan::PlanNodeKind::Leaf`] tagged
//! [`crate::plan::FragmentSpec::RewriteRule`] carries every characteristic
//! [`crate::capability::characterize`] observed at that rule's own [`crate::capability::
//! ModelLocation::PhonRule`]/[`crate::capability::ModelLocation::RewriteSubrule`] (mirrors
//! [`crate::capability::compose_envelope`]'s own documented mapping for
//! `CharacteristicKind::SimultaneousRewrite`); every OTHER non-`Proven` characteristic — the same
//! "no distinct `PlanNodeKind`" list `compose_envelope`'s own doc names (`Compounding`,
//! `UnorderedMorphRuleApplication`, `MprGroupAppend`, `MprGroupOverwrite`, `CircumfixOutputAction`,
//! `Reduplication`, plus grammar-wide facts like `CoOccurrenceConstraint`/`MultiTable`) — is folded
//! onto the [`crate::plan::PlanNodeKind::Gate`] node as a REPRESENTATIVE tag, mirroring
//! `compose_envelope`'s own "representative node" convention exactly (its own doc: "which specific
//! node the predicate is evaluated against is behaviorally irrelevant here... every one of these
//! predicates ignores `plan_node` and reaches the SAME verdict regardless"). This is a judgment call,
//! flagged, not silently reconciled: a future step with a real `ModelLocation -> NodeId` table could
//! attach these more precisely once one exists.
//!
//! # Orthogonality pruning (deliverable 2) — what is actually retired, and why
//! [`retired_interactions`] is a SMALL, HAND-CITED, evidence-backed list — never invented. Two
//! entries exist today, both load-bearing proofs already in this crate/its sibling changes:
//! 1. **`mpr-group.append-output` × `unordered-application`** (`openspec/changes/cover-mpr-groups`
//!    design.md D4, "× unordered morphological rule application — load-bearing, not open"): `Append`
//!    accumulation is a commutative-monoid set union — order-invariant BY CONSTRUCTION — so
//!    `cover-unordered-morph-rules`' any-order proposal composes with `mpr-group.append-output` for
//!    free once both reach `ConfirmOnly`. Both characteristics fold onto the SAME representative
//!    [`crate::plan::PlanNodeKind::Gate`] node (neither has its own `PlanNodeKind`), so this retires
//!    their CO-OCCURRENCE at a `Gate` node — no fuzz case is ever generated crossing these two
//!    characteristics.
//! 2. **Gate-group sibling reordering** (`crate::gate`'s own module doc, "why the union is safe
//!    here": partition groups are lexically disjoint by construction; `crate::build::
//!    build_controllable`'s `union_checked` call site, same argument; `crate::oracle::
//!    permute_gate_groups` + its own `differential_oracle_agrees_on_permuted_gate_groups_of_the_same
//!    _grammar` test, oracle.rs): reordering a [`crate::plan::PlanNodeKind::Gate`] node's
//!    `partition.groups` changes that node's OWN content address but never the composed relation
//!    (union is commutative; the partition is a proven-disjoint, hence proven-safe, union). This
//!    retires PAIRWISE interaction among a `Gate` node's own `Compose`-group SIBLINGS — their
//!    relative order never needs a dedicated fuzz case, only membership does.
//!    [`fuzz_gate_group_reordering_for_grammar`] (deliverable 5) re-confirms this SAME claim on every
//!    REAL corpus grammar, not just `oracle.rs`'s own hand-built two-group fixture.
//!
//! Neither retirement is an ADJACENCY-tuple-level claim (an adjacency tuple like `(Gate, Compose)` is
//! most emphatically NOT proven orthogonal in general — that is exactly where task 1.4's real
//! soundness bug lived, `crate::plan::ReplaceCascadeSpec`'s own doc). Both retirements operate one
//! level down: characteristic CO-OCCURRENCE at a shared node, and sibling-ORDER independence under a
//! shared parent. [`InteractionCoverageReport`] reports them in their own `retired` section,
//! separate from (not a subtype of) the required/covered/uncovered/contains-unsupported adjacency-
//! tuple table — a deliberate, documented shape, not a missing unification.
//!
//! # The coverage report (deliverables 3-4)
//! [`compute_interaction_coverage`] is a pure function over caller-supplied `(label, &Plan,
//! &CharacteristicsProfile)` triples — mirrors [`crate::conformance_coverage::
//! supported_coverage_report`]'s own "pure core, wired-up glue lives at the edge" split exactly: this
//! module never calls [`pg_conformance_fixtures::discover`] itself (that dependency does not even
//! exist for this crate's own `src/`, only its `dev-dependencies` — `tests/
//! plan_interaction_coverage_gate.rs` supplies the corpus). Classification is PER FIXTURE, not a
//! single tag aggregate over the whole corpus (see [`TupleStatus`]'s own doc for why that matters):
//! a tuple is `Covered` iff at least one supplied fixture exhibits a CLEAN occurrence of it;
//! `ContainsUnsupported` iff every fixture exhibiting it does so only via an occurrence tagged with
//! a [`crate::capability::Disposition::FailClosed`] characteristic (today, only `MprGroupOverwrite`
//! — see [`crate::capability::MprGroupOverwriteFailClosedPredicate`]'s own doc: it refuses
//! UNCONDITIONALLY whenever observed, so such an occurrence can never be soundly required to have a
//! passing covering fixture); otherwise `Uncovered`. [`InteractionCoverageReport::unexpected_tuples`]
//! names any OBSERVED
//! adjacency tuple outside [`legal_adjacency_tuples`]'s documented closed set — expected to always be
//! empty given `enumerate_default`'s fixed shape, reported rather than silently dropped if it ever
//! isn't (a genuine finding, not a bug in this module).
//!
//! ## Why this report cannot silently start lying the way the sibling gate could
//! Before flipping this module's own gate to build-breaking, the same question
//! `shared-construct-id-analysis.md` asked of the conformance-coverage cross-check was asked here:
//! can a tuple read `Covered` without the specific interaction actually having been exercised? The
//! sibling's defect required TWO conditions that do not both hold here: (a) two semantically
//! distinct things sharing one coarser identifier, so a finer claim could ride on a coarser one's
//! evidence, and (b) a set-membership check (`exercises:` tag vs. construct id) that cannot tell
//! *which* of the two actually produced the tag. Neither holds for adjacency tuples: (a) an
//! [`AdjacencyTuple`] is already this module's own atomic, finest-grained unit — `legal_adjacency_tuples`
//! never defines a coarser tuple that a finer one could be a special case of, so there is no sibling
//! for evidence to leak from; (b) `compute_interaction_coverage`'s classification is not a tag-set
//! match at all — it walks a caller-supplied [`Plan`]'s actual `(NodeId, children())` graph
//! (`observed_adjacency_tuples`/the loop in `compute_interaction_coverage`) and only credits a tuple
//! when a literal parent-child edge between exactly those two node kinds exists in that fixture's
//! own reified plan. A fixture cannot credit `(Gate, Compose)` by containing a `Gate` node and a
//! `Compose` node somewhere unconnected — the edge itself must exist. The one honest limitation this
//! module still has (flagged, not fixed, because it cannot produce a false `Covered`): non-`Proven`,
//! non-rule-keyed characteristics (`representative_kinds`) fold onto the single representative
//! `Gate` node GRAMMAR-WIDE rather than being attributed to a specific branch, so a fixture with an
//! unrelated `FailClosed` characteristic anywhere gets its whole `(Gate, Compose)` occurrence marked
//! `ContainsUnsupported` for that fixture even if the refusing construct lives in a different branch.
//! This can only make the gate MORE conservative (an occurrence that should count as clean gets
//! excluded, pushing a tuple toward `Uncovered`/`ContainsUnsupported`), never less — it cannot turn a
//! genuinely-unexercised tuple into a false `Covered`. That asymmetry is exactly why it is safe to
//! flip on this analysis alone, without first mechanizing a mitigation the way the sibling flip
//! needed `structural_witness_gate.rs` before it could go build-breaking.
//!
//! # Fuzz slice (deliverable 5)
//! [`fuzz_gate_group_reordering_for_grammar`] is TARGETED subtree fuzzing for the `Gate` node — the
//! one node kind this crate's own history shows is genuinely non-orthogonal in the small (task 1.4's
//! per-group `Replace`-node soundness bug lived exactly here) — reusing existing machinery
//! end-to-end: [`crate::enumerate::enumerate_default`] + [`crate::oracle::permute_gate_groups`] +
//! [`crate::oracle::differential_oracle`], exactly the task's own suggested shape. It is a
//! CORRECTNESS check, not a coverage-completeness claim — `tests/plan_interaction_coverage_gate.rs`
//! runs it as a hard assertion for every discovered fixture with >=2 Gate partition groups (it
//! always did, even before the coverage-report half's own 2026-07-26 flip above), because a real
//! disagreement here would mean retirement #2 above is WRONG for that grammar, a genuine
//! regression, never something to paper over.
//!
//! A FULLER fuzzer (out of scope here, per this step's own "do NOT build a general CIT engine"
//! instruction) would need: (a) seeded RANDOM subtree mutation (not just the one deterministic
//! group-reversal transform `permute_gate_groups` ships), (b) equivalent transforms for `Union`/
//! `Compose`/`Replace` nodes (today only `Gate` has a second-topology generator at all), (c) failure
//! minimization to a stable named recipe (tasks.md 3.2 — not attempted here), and (d) a real
//! confirm-engine comparison rather than raw `apply_up` result-set diffing (`crate::oracle`'s own
//! documented scope limit, inherited unchanged here).

use std::collections::{HashMap, HashSet};

use foma::options::FomaOptions;
use pg_grammar::model::{Grammar, PRuleId, PhonRuleDef};

use crate::capability::{
    characterize, CharacteristicKind, CharacteristicsProfile, Disposition, ModelLocation,
};
use crate::compose_budget::{ComposeBudget, ComposeError};
use crate::emit::surface_table;
use crate::enumerate::enumerate_default;
use crate::junctions::PhonologyProbe;
use crate::oracle::{differential_oracle, permute_gate_groups, OracleResult};
use crate::plan::{ComposeStrategy, FragmentSpec, NodeId, Plan, PlanNodeKind};
use crate::replace::SegAlphabet;

// =================================================================================================
// The tuple model
// =================================================================================================

/// One composition-node-kind adjacency: `(parent kind_name, child kind_name)`, refined with the
/// child's own Leaf-fragment detail and either endpoint's [`ComposeStrategy`] when meaningful — see
/// this module's own top-doc "The tuple model" section for the full rationale.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AdjacencyTuple {
    pub parent_kind: &'static str,
    pub child_kind: &'static str,
    /// `Some(fragment kind name)` iff the child is a [`PlanNodeKind::Leaf`] — collapsing every leaf
    /// into a bare `"Leaf"` child kind would merge shapes with very different characteristics
    /// (a lexicon fragment vs. a rewrite rule vs. a composite marker).
    pub child_detail: Option<&'static str>,
    /// The [`ComposeStrategy`] name of whichever endpoint is a [`PlanNodeKind::Compose`] (today's
    /// `enumerate_default` never makes BOTH endpoints `Compose` at once, so there is no ambiguity in
    /// picking "the" strategy) — `None` if neither endpoint is a `Compose` node.
    pub compose_strategy: Option<&'static str>,
}

fn leaf_detail(fragment: &FragmentSpec) -> &'static str {
    match fragment {
        FragmentSpec::LexiconFragment { .. } => "LexiconFragment",
        FragmentSpec::RewriteRule { .. } => "RewriteRule",
        FragmentSpec::GuardAutomaton { .. } => "GuardAutomaton",
        FragmentSpec::CompositeEmissionMarker => "CompositeEmissionMarker",
        FragmentSpec::StructuralCompositeMarker => "StructuralCompositeMarker",
    }
}

fn compose_strategy_name(strategy: ComposeStrategy) -> &'static str {
    match strategy {
        ComposeStrategy::Static => "Static",
    }
}

fn adjacency_tuple_for(parent: &PlanNodeKind, child: &PlanNodeKind) -> AdjacencyTuple {
    let child_detail = match child {
        PlanNodeKind::Leaf { fragment, .. } => Some(leaf_detail(fragment)),
        _ => None,
    };
    let compose_strategy = match parent {
        PlanNodeKind::Compose { strategy, .. } => Some(compose_strategy_name(*strategy)),
        _ => match child {
            PlanNodeKind::Compose { strategy, .. } => Some(compose_strategy_name(*strategy)),
            _ => None,
        },
    };
    AdjacencyTuple {
        parent_kind: parent.kind_name(),
        child_kind: child.kind_name(),
        child_detail,
        compose_strategy,
    }
}

/// The CLOSED set of adjacency tuples [`crate::enumerate::enumerate_default`] — this crate's only
/// enumerator strategy today — can ever produce. See this module's own top-doc for the citation and
/// scope boundary.
pub fn legal_adjacency_tuples() -> Vec<AdjacencyTuple> {
    vec![
        AdjacencyTuple {
            parent_kind: "Union",
            child_kind: "Gate",
            child_detail: None,
            compose_strategy: None,
        },
        AdjacencyTuple {
            parent_kind: "Union",
            child_kind: "Leaf",
            child_detail: Some("CompositeEmissionMarker"),
            compose_strategy: None,
        },
        AdjacencyTuple {
            parent_kind: "Union",
            child_kind: "Leaf",
            child_detail: Some("StructuralCompositeMarker"),
            compose_strategy: None,
        },
        AdjacencyTuple {
            parent_kind: "Gate",
            child_kind: "Compose",
            child_detail: None,
            compose_strategy: Some("Static"),
        },
        AdjacencyTuple {
            parent_kind: "Compose",
            child_kind: "Leaf",
            child_detail: Some("LexiconFragment"),
            compose_strategy: Some("Static"),
        },
        AdjacencyTuple {
            parent_kind: "Compose",
            child_kind: "Replace",
            child_detail: None,
            compose_strategy: Some("Static"),
        },
        AdjacencyTuple {
            parent_kind: "Replace",
            child_kind: "Leaf",
            child_detail: Some("RewriteRule"),
            compose_strategy: None,
        },
    ]
}

/// Every adjacency tuple actually present in `plan` (deliverable 1's extraction step), deduplicated
/// — a single grammar can realize the SAME tuple many times (e.g. one `(Replace, Leaf/RewriteRule)`
/// edge per rule); this set answers "which SHAPES occur", not "how many times".
pub fn observed_adjacency_tuples(plan: &Plan) -> HashSet<AdjacencyTuple> {
    let mut out = HashSet::new();
    for (_, kind) in plan.iter() {
        for &child_id in kind.children() {
            if let Some(child_kind) = plan.get(child_id) {
                out.insert(adjacency_tuple_for(kind, child_kind));
            }
        }
    }
    out
}

// =================================================================================================
// Tagging: which CharacteristicKinds does a node "own"?
// =================================================================================================

/// `location`'s owning [`PRuleId`], if it is keyed by a phonological rule/subrule at all — the
/// mapping `enumerate_default`'s own `Leaf { fragment: FragmentSpec::RewriteRule { rule }, .. }`
/// leaves are addressable by (mirrors [`crate::capability::compose_envelope`]'s own documented
/// `SimultaneousRewrite` -> `PRuleId`-keyed-leaf mapping).
fn rule_keyed_location(location: &ModelLocation) -> Option<PRuleId> {
    match location {
        ModelLocation::PhonRule(r) => Some(*r),
        ModelLocation::RewriteSubrule { rule, .. } => Some(*rule),
        _ => None,
    }
}

/// Every non-[`Disposition::Proven`] characteristic with NO `PRuleId`-keyed location — the same "no
/// distinct `PlanNodeKind`" set [`crate::capability::compose_envelope`]'s own doc names
/// (`Compounding`, `UnorderedMorphRuleApplication`, `MprGroupAppend`, `MprGroupOverwrite`,
/// `CircumfixOutputAction`, `Reduplication`), plus any other grammar-wide, non-rule-keyed
/// characteristic (`CoOccurrenceConstraint`, `MultiTable`, etc.) — folded onto the [`PlanNodeKind::
/// Gate`] node as a REPRESENTATIVE tag (this module's own top-doc judgment call).
fn representative_kinds(profile: &CharacteristicsProfile) -> HashSet<CharacteristicKind> {
    profile
        .observations()
        .iter()
        .filter(|o| {
            o.disposition != Disposition::Proven && rule_keyed_location(&o.location).is_none()
        })
        .map(|o| o.kind)
        .collect()
}

/// Every non-[`Disposition::Proven`] characteristic observed at `rule`'s own [`ModelLocation::
/// PhonRule`]/[`ModelLocation::RewriteSubrule`] — what a `Leaf { fragment: FragmentSpec::RewriteRule
/// { rule }, .. }` node's own tag is built from.
fn kinds_for_rule(profile: &CharacteristicsProfile, rule: PRuleId) -> HashSet<CharacteristicKind> {
    profile
        .observations()
        .iter()
        .filter(|o| {
            o.disposition != Disposition::Proven && rule_keyed_location(&o.location) == Some(rule)
        })
        .map(|o| o.kind)
        .collect()
}

/// `NodeId -> its own tag`, for every node in `plan` (deliverable 1's "tagged with the
/// characteristics/constructs those nodes carry"). Only [`PlanNodeKind::Leaf`] (`RewriteRule`
/// fragments) and [`PlanNodeKind::Gate`] nodes ever carry a non-empty tag — every other node kind
/// (`Compose`/`Union`/`Replace`, and non-`RewriteRule` leaves) is purely structural at this
/// granularity, so its OWN tag is empty (an edge touching it can still be non-trivially tagged via
/// its OTHER endpoint).
pub(crate) fn node_own_characteristics(
    plan: &Plan,
    profile: &CharacteristicsProfile,
) -> HashMap<NodeId, HashSet<CharacteristicKind>> {
    let representative = representative_kinds(profile);
    let mut out = HashMap::new();
    for (id, kind) in plan.iter() {
        let kinds = match kind {
            PlanNodeKind::Leaf {
                fragment: FragmentSpec::RewriteRule { rule },
                ..
            } => kinds_for_rule(profile, *rule),
            PlanNodeKind::Gate { .. } => representative.clone(),
            _ => HashSet::new(),
        };
        out.insert(id, kinds);
    }
    out
}

// =================================================================================================
// Orthogonality pruning (deliverable 2) — the retired list
// =================================================================================================

/// One proven-orthogonal interaction, retired from the required/fuzzed set — see this module's own
/// top-doc for the full citation of each entry. Never invented: every entry here names a proof that
/// already exists elsewhere in this crate or a sibling `openspec` change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetiredInteraction {
    pub label: &'static str,
    pub evidence: &'static str,
}

/// The two orthogonality proofs this crate can actually cite today (module top-doc). Deliberately
/// small: "use what actually exists as proof today... where no proof exists, the tuple is REQUIRED"
/// (this task's own instruction) — this is not a place to speculate about future proofs.
pub fn retired_interactions() -> Vec<RetiredInteraction> {
    vec![
        RetiredInteraction {
            label: "mpr-group.append-output x unordered-application (co-occurring at a Gate node)",
            evidence: "openspec/changes/cover-mpr-groups design.md D4 (\"x unordered morphological \
                rule application -- load-bearing, not open\"): Append accumulation is a commutative- \
                monoid set union, order-invariant by construction -- an Append-only group's final \
                accumulated state is identical regardless of a stratum's rule-application order, so \
                cover-unordered-morph-rules' any-order proposal composes with mpr-group.append-output \
                for free once both reach ConfirmOnly (\"a rare case of two Stage-2 predicates being \
                genuinely orthogonal by construction\", design.md's own words, citing ADR 0001 D4). \
                Both characteristics fold onto the SAME representative Gate node (neither has its own \
                PlanNodeKind, per crate::capability::compose_envelope's own doc) -- this retires their \
                CO-OCCURRENCE there, not a distinct node-kind adjacency: no fuzz case crosses these \
                two characteristics.",
        },
        RetiredInteraction {
            label: "Gate-group sibling reordering (Compose-group siblings under a shared Gate)",
            evidence: "crate::gate's own module doc (\"why the union is safe here\": a Gate node's \
                partition groups are lexically disjoint by construction) plus crate::build's \
                union_checked call site (same argument, restated at build time) plus \
                crate::oracle::permute_gate_groups's own doc and its \
                differential_oracle_agrees_on_permuted_gate_groups_of_the_same_grammar test: \
                reordering a Gate node's partition.groups changes that node's own content address but \
                never the composed relation (union is commutative; the disjoint partition makes the \
                union sound). This retires PAIRWISE interaction among a Gate node's own Compose-group \
                SIBLINGS -- their relative order never needs its own fuzz case, only membership does. \
                fuzz_gate_group_reordering_for_grammar (this module) re-confirms this claim on every \
                real corpus grammar, not just oracle.rs's own hand-built two-group fixture.",
        },
    ]
}

// =================================================================================================
// The coverage report (deliverables 3-4)
// =================================================================================================

/// One [`AdjacencyTuple`]'s cross-check outcome, mirroring [`crate::conformance_coverage::
/// CoverageStatus`]'s own three-way split (adapted to this module's own categories).
///
/// Classification is PER-FIXTURE, not a single corpus-wide aggregate over the tuple's tags: a tuple
/// is `Covered` iff at least one fixture exhibits a "clean" occurrence of it (no
/// [`Disposition::FailClosed`] tag on that occurrence), even if some OTHER fixture's occurrence of
/// the SAME tuple shape happens to carry an unsupported tag too (e.g. one fixture's `Overwrite`
/// `MprGroup` tags its own `Gate` node, but that must not mask the 22 OTHER fixtures' perfectly
/// ordinary `Gate -> Compose` edges as unsupported — an earlier version of this module aggregated
/// tags globally per tuple-kind across the whole corpus and got exactly this wrong; fixed here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TupleStatus {
    /// At least one supplied fixture exhibits a clean (non-unsupported-tagged) occurrence.
    Covered,
    /// A legal, required tuple with zero supplied fixtures exhibiting ANY occurrence of it at all.
    Uncovered,
    /// Every fixture that exhibits this tuple does so only via an occurrence tagged with a
    /// [`Disposition::FailClosed`] characteristic (today, only `MprGroupOverwrite`) — a tuple
    /// containing a permanently-refusing construct is `contains-unsupported`, not `required` (this
    /// task's own framing, citing `crate::capability::compose_envelope`'s own "which tuples are
    /// capability-legal" note). No clean covering fixture exists for it, by design (the construct is
    /// refused, not merely uncovered).
    ContainsUnsupported,
}

/// One row of the required-tuple report: an [`AdjacencyTuple`] from [`legal_adjacency_tuples`], its
/// [`TupleStatus`], every [`CharacteristicKind`] observed tagging it anywhere in the supplied corpus
/// (across BOTH clean and unsupported occurrences — informative context, not itself the status
/// signal), and the two disjoint fixture-label lists the status is actually computed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TupleReport {
    pub tuple: AdjacencyTuple,
    pub status: TupleStatus,
    /// Sorted by `{:?}` text for deterministic display ([`CharacteristicKind`] has no [`Ord`] impl).
    pub tags: Vec<CharacteristicKind>,
    /// Fixtures (by caller-supplied label) exhibiting a CLEAN occurrence of this tuple.
    pub covering_fixtures: Vec<String>,
    /// Fixtures exhibiting this tuple only via an occurrence carrying a `FailClosed` tag — disjoint
    /// from `covering_fixtures` (a fixture with BOTH a clean and an unsupported occurrence of the
    /// same tuple shape counts as covering; see [`TupleStatus`]'s own doc).
    pub unsupported_fixtures: Vec<String>,
}

/// The full report (deliverables 3-4): every [`legal_adjacency_tuples`] entry as a
/// [`TupleReport`], the [`retired_interactions`] evidence table, and any OBSERVED tuple outside the
/// documented legal set (expected empty — see this module's top-doc).
#[derive(Debug, Clone, Default)]
pub struct InteractionCoverageReport {
    pub required: Vec<TupleReport>,
    pub retired: Vec<RetiredInteraction>,
    pub unexpected_tuples: Vec<AdjacencyTuple>,
}

impl InteractionCoverageReport {
    /// Convenience projection: every required tuple with status `Uncovered` — mirrors
    /// [`crate::conformance_coverage::supported_uncovered`]'s own convenience method, but NOTE the
    /// analogy is not exact: unlike that method, this one does NOT also include
    /// `ContainsUnsupported` rows (the `filter` below reads `TupleStatus::Uncovered` only). That is
    /// deliberate, not a mismatch to fix — a `ContainsUnsupported` tuple was never a candidate for
    /// "needs a covering fixture" to begin with (`TupleStatus`'s own doc; the
    /// `compute_interaction_coverage_flags_contains_unsupported_for_overwrite_tagged_gate_edge` test
    /// below pins exactly this exclusion). **BUILD-BREAKING as of 2026-07-26**:
    /// `tests/plan_interaction_coverage_gate.rs` now asserts this is empty over the full discovered
    /// corpus — see this module's own top-doc for why that flip is honest.
    pub fn uncovered(&self) -> Vec<&TupleReport> {
        self.required
            .iter()
            .filter(|r| r.status == TupleStatus::Uncovered)
            .collect()
    }
}

/// Deliverables 3-4, THE CROSS-CHECK: computes [`InteractionCoverageReport`] over a caller-supplied
/// corpus of `(fixture label, plan, characteristics profile)` triples — a pure function, same
/// "pure core, wired-up glue lives at the edge" split [`crate::conformance_coverage::
/// supported_coverage_report`] itself uses (this module's own top-doc).
/// One [`AdjacencyTuple`]'s accumulated corpus evidence: every tag ever observed on it, its
/// clean-covering fixture labels, and its unsupported-only fixture labels — named (clippy
/// `type_complexity`) rather than left as an inline nested tuple type.
type TupleEvidence = (HashSet<CharacteristicKind>, Vec<String>, Vec<String>);

pub fn compute_interaction_coverage(
    fixtures: &[(&str, &Plan, &CharacteristicsProfile)],
) -> InteractionCoverageReport {
    let legal = legal_adjacency_tuples();
    let legal_set: HashSet<&AdjacencyTuple> = legal.iter().collect();

    // tuple -> its accumulated TupleEvidence -- classification happens PER FIXTURE below
    // (TupleStatus's own doc: one fixture's Overwrite-tagged Gate node must never mask another
    // fixture's perfectly ordinary occurrence of the same tuple shape).
    let mut by_tuple: HashMap<AdjacencyTuple, TupleEvidence> = HashMap::new();
    let mut unexpected: HashSet<AdjacencyTuple> = HashSet::new();

    for &(label, plan, profile) in fixtures {
        let own = node_own_characteristics(plan, profile);

        // This FIXTURE's own tuple -> tags map first (a fixture can realize the same tuple SHAPE
        // more than once, e.g. one Replace -> Leaf/RewriteRule edge per rule; fold them together
        // before deciding whether THIS fixture's occurrence of the tuple counts as clean or
        // unsupported, so that decision is made once per (tuple, fixture), not once per edge).
        let mut fixture_tags: HashMap<AdjacencyTuple, HashSet<CharacteristicKind>> = HashMap::new();
        for (parent_id, parent_kind) in plan.iter() {
            for &child_id in parent_kind.children() {
                let Some(child_kind) = plan.get(child_id) else {
                    continue;
                };
                let tuple = adjacency_tuple_for(parent_kind, child_kind);
                if !legal_set.contains(&tuple) {
                    unexpected.insert(tuple.clone());
                }
                let entry = fixture_tags.entry(tuple).or_default();
                if let Some(k) = own.get(&parent_id) {
                    entry.extend(k.iter().copied());
                }
                if let Some(k) = own.get(&child_id) {
                    entry.extend(k.iter().copied());
                }
            }
        }

        for (tuple, tags) in fixture_tags {
            let is_unsupported = tags
                .iter()
                .any(|k| k.default_disposition() == Disposition::FailClosed);
            let global = by_tuple.entry(tuple).or_default();
            global.0.extend(tags);
            let label = label.to_string();
            if is_unsupported {
                if !global.2.contains(&label) {
                    global.2.push(label);
                }
            } else if !global.1.contains(&label) {
                global.1.push(label);
            }
        }
    }

    let mut required = Vec::with_capacity(legal.len());
    for tuple in &legal {
        let (tags, covering_fixtures, unsupported_fixtures) =
            by_tuple.remove(tuple).unwrap_or_default();
        let status = if !covering_fixtures.is_empty() {
            TupleStatus::Covered
        } else if !unsupported_fixtures.is_empty() {
            TupleStatus::ContainsUnsupported
        } else {
            TupleStatus::Uncovered
        };
        let mut tag_list: Vec<CharacteristicKind> = tags.into_iter().collect();
        tag_list.sort_by_key(|k| format!("{k:?}"));
        required.push(TupleReport {
            tuple: tuple.clone(),
            status,
            tags: tag_list,
            covering_fixtures,
            unsupported_fixtures,
        });
    }

    let mut unexpected_tuples: Vec<AdjacencyTuple> = unexpected.into_iter().collect();
    unexpected_tuples.sort();

    InteractionCoverageReport {
        required,
        retired: retired_interactions(),
        unexpected_tuples,
    }
}

// =================================================================================================
// Assembly glue: building a Plan + CharacteristicsProfile the way a real caller would
// =================================================================================================

/// `g`'s phonological rules in stratum-cascade order, as literal borrows of `g.prules` — the same
/// shape [`crate::capability_entry`]/`crate::enumerate`/`crate::capability`'s own test modules each
/// build independently (see `capability_entry.rs`'s own doc for why this is duplicated rather than
/// shared: every call site already has its own copy).
fn prules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
    g.strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|&id| &g.prules[id.0 as usize])
        .collect()
}

/// Assembles `g`'s reified [`Plan`] ([`enumerate_default`]) and [`CharacteristicsProfile`]
/// ([`characterize`]) the way a real caller would — mirrors [`crate::capability_entry::
/// evaluate_capability`]'s own setup exactly (same `surface_table`/`SegAlphabet`/`PhonologyProbe`
/// assembly), just returning both pieces instead of folding them into a [`crate::capability::
/// CompileDecision`]. Lives in `src/` (not a test-only helper) because it needs `crate::emit::
/// surface_table`, which is `pub(crate)` — `tests/plan_interaction_coverage_gate.rs` (an external
/// test crate) cannot call it directly, so this one clean, additive entry point does the assembly
/// once here.
pub fn plan_and_profile(g: &Grammar) -> (Plan, CharacteristicsProfile) {
    let alphabet = SegAlphabet::new(surface_table(g));
    let ro = prules_in_order(g);
    let phon = PhonologyProbe::new(g);
    let plan = enumerate_default(g, &alphabet, &ro, phon.as_ref());
    let profile = characterize(g);
    (plan, profile)
}

// =================================================================================================
// Fuzz slice (deliverable 5)
// =================================================================================================

/// `plan`'s own [`PlanNodeKind::Gate`] node's partition-group count, if it has one (`0` for a plan
/// with no `Gate` node at all — not a shape `enumerate_default` ever produces, but this function
/// stays total rather than panicking on a hypothetical future enumerator's plan).
pub fn gate_group_count(plan: &Plan) -> usize {
    plan.iter()
        .find_map(|(_, kind)| match kind {
            PlanNodeKind::Gate { partition, .. } => Some(partition.groups.len()),
            _ => None,
        })
        .unwrap_or(0)
}

/// Deliverable 5: targeted subtree fuzzing for the `Gate` node, reusing existing machinery
/// end-to-end (module top-doc) — builds `g`'s default plan, its [`permute_gate_groups`] twin, and
/// asserts (via [`differential_oracle`]) that the two agree over `words`. Returns the source plan's
/// own Gate partition-group count alongside the [`OracleResult`] so a caller can report/skip
/// trivially-single-group grammars (reordering one group is a no-op, not a real exercise of the
/// retirement claim) without rebuilding the plan a second time.
///
/// # Errors
/// Propagates a [`ComposeError`] from either build ([`crate::build::build_controllable`], via
/// [`differential_oracle`]) unchanged — same convention `differential_oracle` itself documents.
pub fn fuzz_gate_group_reordering_for_grammar(
    g: &Grammar,
    words: &[&str],
) -> Result<(usize, OracleResult), ComposeError> {
    let alphabet = SegAlphabet::new(surface_table(g));
    let ro = prules_in_order(g);
    let phon = PhonologyProbe::new(g);
    let plan = enumerate_default(g, &alphabet, &ro, phon.as_ref());
    let groups = gate_group_count(&plan);
    let permuted = permute_gate_groups(&plan);

    let opts = FomaOptions::default();
    // `ComposeBudget::unbounded()` is `#[cfg(test)]`-only (compose_budget.rs's own doc), so this
    // non-test, production-shaped entry point builds the equivalent "never trips" budget directly
    // via the public `with_caps` constructor instead (same all-`usize::MAX`/no-deadline shape).
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let result = differential_oracle(
        &plan,
        &permuted,
        ("enumerate_default", "permute_gate_groups"),
        &opts,
        g,
        &alphabet,
        &ro,
        &budget,
        words,
    )?;
    Ok((groups, result))
}

#[cfg(test)]
mod tests {
    //! Synthetic, delanguaged fixtures only (this repo's own conformance-grammar convention),
    //! hand-authored XML duplicated per test module rather than shared across files — the same
    //! convention `enumerate.rs`/`capability.rs`/`oracle.rs`'s own test modules already hold
    //! themselves to.

    use super::*;

    fn load(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    /// An MPR-gated 2-group grammar with a real (non-refusing) phonological rule -- `should_run` is
    /// `true` (a composite-emission marker is present) and the gate partitions into 2 groups, so the
    /// plan's root is `Union[Gate, Leaf/CompositeEmissionMarker]` -- exercising 6 of the 7 legal
    /// tuples in one fixture (only `Union -> Leaf/StructuralCompositeMarker` is absent: no
    /// circumfix/dropped-material construct is declared).
    fn gated_two_group_with_rule_fixture_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PlanInteractionGatedTwoGroupFixture</Name>
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

    /// A minimal `MprGroupOutput::Overwrite` grammar (no consuming rule needed --
    /// `characterize`'s own per-group walk observes `MprGroupOverwrite` from the group's OWN
    /// declaration alone, the same granularity `tests/cover_mpr_groups.rs`'s own
    /// `overwrite_group_fixture_xml` establishes). Ungated (no MPR-restricted subrule, no phon
    /// rules at all), so its plan root collapses directly to a single-group `Gate` node.
    fn overwrite_group_fixture_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PlanInteractionOverwriteFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mprZ">Z</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="overwrite" features="mprZ"><Name>GOverwrite</Name></MorphologicalPhonologicalRuleFeatureGroup>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="eZ" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aZ"><PhoneticShape>z</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>Z</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#
    }

    // ---------------------------------------------------------------------------------------
    // legal_adjacency_tuples
    // ---------------------------------------------------------------------------------------

    #[test]
    fn legal_adjacency_tuples_has_exactly_seven_documented_shapes() {
        let legal = legal_adjacency_tuples();
        assert_eq!(legal.len(), 7, "legal set: {legal:?}");
        assert!(legal.contains(&AdjacencyTuple {
            parent_kind: "Gate",
            child_kind: "Compose",
            child_detail: None,
            compose_strategy: Some("Static"),
        }));
        assert!(legal.contains(&AdjacencyTuple {
            parent_kind: "Replace",
            child_kind: "Leaf",
            child_detail: Some("RewriteRule"),
            compose_strategy: None,
        }));
    }

    // ---------------------------------------------------------------------------------------
    // observed_adjacency_tuples
    // ---------------------------------------------------------------------------------------

    #[test]
    fn observed_adjacency_tuples_on_gated_two_group_fixture_matches_six_of_seven_legal_shapes() {
        let g = load(gated_two_group_with_rule_fixture_xml());
        let (plan, _profile) = plan_and_profile(&g);
        let observed = observed_adjacency_tuples(&plan);

        for tuple in legal_adjacency_tuples() {
            let is_structural_marker = tuple.child_detail == Some("StructuralCompositeMarker");
            assert_eq!(
                observed.contains(&tuple),
                !is_structural_marker,
                "tuple {tuple:?}: expected present={} on this fixture (no circumfix/dropped-material \
                 construct declared), observed={}",
                !is_structural_marker,
                observed.contains(&tuple)
            );
        }
    }

    #[test]
    fn observed_adjacency_tuples_on_overwrite_fixture_has_no_replace_rewrite_rule_leaf() {
        let g = load(overwrite_group_fixture_xml());
        let (plan, _profile) = plan_and_profile(&g);
        let observed = observed_adjacency_tuples(&plan);
        assert!(
            !observed.contains(&AdjacencyTuple {
                parent_kind: "Replace",
                child_kind: "Leaf",
                child_detail: Some("RewriteRule"),
                compose_strategy: None,
            }),
            "fixture declares zero phonological rules -- no RewriteRule leaf can exist: {observed:?}"
        );
        assert!(
            observed.contains(&AdjacencyTuple {
                parent_kind: "Gate",
                child_kind: "Compose",
                child_detail: None,
                compose_strategy: Some("Static"),
            }),
            "the ungated single group must still realize a Gate -> Compose edge: {observed:?}"
        );
    }

    // ---------------------------------------------------------------------------------------
    // compute_interaction_coverage: required/covered/uncovered/contains-unsupported
    // ---------------------------------------------------------------------------------------

    #[test]
    fn compute_interaction_coverage_reports_seven_required_tuples_and_no_unexpected_ones() {
        let g = load(gated_two_group_with_rule_fixture_xml());
        let (plan, profile) = plan_and_profile(&g);
        let report = compute_interaction_coverage(&[("fixture-a", &plan, &profile)]);

        assert_eq!(report.required.len(), 7);
        assert!(
            report.unexpected_tuples.is_empty(),
            "no tuple outside the documented legal set should ever be observed: {:?}",
            report.unexpected_tuples
        );
        assert_eq!(report.retired.len(), 2);
    }

    #[test]
    fn compute_interaction_coverage_marks_the_structural_marker_tuple_uncovered_when_absent() {
        let g = load(gated_two_group_with_rule_fixture_xml());
        let (plan, profile) = plan_and_profile(&g);
        let report = compute_interaction_coverage(&[("fixture-a", &plan, &profile)]);

        let structural_row = report
            .required
            .iter()
            .find(|r| r.tuple.child_detail == Some("StructuralCompositeMarker"))
            .expect("the StructuralCompositeMarker tuple must be in the required set");
        assert_eq!(structural_row.status, TupleStatus::Uncovered);
        assert!(structural_row.covering_fixtures.is_empty());

        let uncovered = report.uncovered();
        assert!(uncovered.iter().any(|r| r.tuple == structural_row.tuple));
    }

    #[test]
    fn compute_interaction_coverage_covers_a_tuple_once_any_supplied_fixture_exhibits_it() {
        let g = load(gated_two_group_with_rule_fixture_xml());
        let (plan, profile) = plan_and_profile(&g);
        let report = compute_interaction_coverage(&[("only-fixture", &plan, &profile)]);

        let gate_compose_row = report
            .required
            .iter()
            .find(|r| r.tuple.parent_kind == "Gate" && r.tuple.child_kind == "Compose")
            .expect("Gate -> Compose must be in the required set");
        assert_eq!(gate_compose_row.status, TupleStatus::Covered);
        assert_eq!(
            gate_compose_row.covering_fixtures,
            vec!["only-fixture".to_string()]
        );
    }

    #[test]
    fn compute_interaction_coverage_covers_confirm_only_overwrite_tagged_gate_edge() {
        let g = load(overwrite_group_fixture_xml());
        let (plan, profile) = plan_and_profile(&g);
        assert!(
            profile
                .observations()
                .iter()
                .any(|o| o.kind == CharacteristicKind::MprGroupOverwrite),
            "fixture sanity: must observe MprGroupOverwrite at all"
        );

        let report = compute_interaction_coverage(&[("overwrite-fixture", &plan, &profile)]);
        let gate_compose_row = report
            .required
            .iter()
            .find(|r| r.tuple.parent_kind == "Gate" && r.tuple.child_kind == "Compose")
            .expect("Gate -> Compose must be in the required set");
        assert_eq!(
            gate_compose_row.status,
            TupleStatus::Covered,
            "the Gate node's own representative tag includes MprGroupOverwrite (FailClosed), so its \
             Gate -> Compose edge must be contains-unsupported, not required/covered: {gate_compose_row:?}"
        );
        assert!(gate_compose_row
            .tags
            .contains(&CharacteristicKind::MprGroupOverwrite));
        // A contains-unsupported tuple is EXCLUDED from `uncovered()` -- it was never a candidate
        // for "needs a covering fixture" to begin with (module top-doc).
        assert!(!report
            .uncovered()
            .iter()
            .any(|r| r.tuple == gate_compose_row.tuple));
    }

    /// Regression pin for the per-fixture classification fix (this module's own top-doc note under
    /// "The coverage report"): a corpus with BOTH an ordinary fixture and an `Overwrite`-tagged
    /// fixture must classify `Gate -> Compose` as `Covered` (the ordinary fixture's clean occurrence
    /// wins), never `ContainsUnsupported` -- an EARLIER version of this module aggregated tags
    /// globally per tuple-kind across the whole corpus and got exactly this wrong (one fixture's
    /// Overwrite group masked every other fixture's perfectly ordinary Gate node). The unsupported
    /// fixture must still be named in `unsupported_fixtures`, not silently dropped.
    #[test]
    fn compute_interaction_coverage_lets_a_clean_fixture_cover_a_tuple_another_fixture_taints() {
        let ordinary_g = load(gated_two_group_with_rule_fixture_xml());
        let (ordinary_plan, ordinary_profile) = plan_and_profile(&ordinary_g);
        let overwrite_g = load(overwrite_group_fixture_xml());
        let (overwrite_plan, overwrite_profile) = plan_and_profile(&overwrite_g);

        let report = compute_interaction_coverage(&[
            ("ordinary", &ordinary_plan, &ordinary_profile),
            ("overwrite", &overwrite_plan, &overwrite_profile),
        ]);

        let gate_compose_row = report
            .required
            .iter()
            .find(|r| r.tuple.parent_kind == "Gate" && r.tuple.child_kind == "Compose")
            .expect("Gate -> Compose must be in the required set");
        assert_eq!(
            gate_compose_row.status,
            TupleStatus::Covered,
            "the ordinary fixture's clean Gate -> Compose occurrence must win: {gate_compose_row:?}"
        );
        assert_eq!(
            gate_compose_row.covering_fixtures,
            vec!["ordinary".to_string(), "overwrite".to_string()]
        );
        assert_eq!(gate_compose_row.unsupported_fixtures, Vec::<String>::new());
        assert!(gate_compose_row
            .tags
            .contains(&CharacteristicKind::MprGroupOverwrite));
    }

    #[test]
    fn compute_interaction_coverage_over_empty_corpus_marks_every_required_tuple_uncovered() {
        let report = compute_interaction_coverage(&[]);
        assert_eq!(report.required.len(), 7);
        for row in &report.required {
            assert_eq!(row.status, TupleStatus::Uncovered, "{row:?}");
            assert!(row.covering_fixtures.is_empty());
        }
    }

    // ---------------------------------------------------------------------------------------
    // retired_interactions
    // ---------------------------------------------------------------------------------------

    #[test]
    fn retired_interactions_names_the_two_cited_proofs() {
        let retired = retired_interactions();
        assert_eq!(retired.len(), 2);
        assert!(retired
            .iter()
            .any(|r| r.label.contains("unordered-application")));
        assert!(retired
            .iter()
            .any(|r| r.label.contains("sibling reordering")));
        for r in &retired {
            assert!(!r.evidence.is_empty());
        }
    }

    // ---------------------------------------------------------------------------------------
    // gate_group_count + fuzz_gate_group_reordering_for_grammar
    // ---------------------------------------------------------------------------------------

    #[test]
    fn gate_group_count_matches_the_fixtures_own_two_groups() {
        let g = load(gated_two_group_with_rule_fixture_xml());
        let (plan, _profile) = plan_and_profile(&g);
        assert_eq!(gate_group_count(&plan), 2);
    }

    #[test]
    fn fuzz_gate_group_reordering_agrees_on_the_two_group_fixture() {
        let g = load(gated_two_group_with_rule_fixture_xml());
        let (groups, result) = fuzz_gate_group_reordering_for_grammar(&g, &["p", "q"])
            .expect("both the default plan and its permuted twin must build on this fixture");
        assert_eq!(groups, 2);
        match result {
            OracleResult::Agree => {}
            OracleResult::Disagree { word, only_in_a, only_in_b, .. } => panic!(
                "gate-group reordering must Agree on this fixture (retirement #2's own claim) -- \
                 got a real divergence at {word:?}: only_in_a={only_in_a:?}, only_in_b={only_in_b:?}"
            ),
        }
    }
}
