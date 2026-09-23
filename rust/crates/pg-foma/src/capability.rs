//! Decides what a compiler strategy may honestly claim to represent for a given grammar.
//!
//! Holds the `CharacteristicsProfile` projection, the `CapabilityPredicate` trait and
//! `PredicateVerdict`, the exhaustive default-deny `characterize`, and the
//! `simultaneous.subrule-overlap` predicate.
//!
//! This gates SELECTION, not COMPILATION: `compose_envelope_for_strategy` decides what
//! `crate::selection` may offer, while `emit`/`gate`/`replace`/`preexpand` compile exactly as
//! they would otherwise. A refusal is reported rather than silently degraded, so an unrepresentable
//! construct never turns into a quietly wrong parse.
//!
//! # The characteristics projection
//! `characterize` walks a `Grammar` and matches **every** variant of **every** frozen
//! `model.rs` enum, with **no catch-all arm** — the discipline that would have caught the
//! `Compounding` silent-recall hole. Adding a new `model.rs`
//! variant to any of those enums breaks THIS file's build until `characterize` (or one of its
//! private per-construct helpers) is updated to give it an explicit `Disposition` — see this
//! module's tests for a from-scratch check of that property against
//! `pg_grammar::model::ReduplicationHint`/`pg_grammar::model::OutputAction`/etc.
//!
//! # The predicate trait + verdict
//! `CapabilityPredicate` is an oracle-verified proof-obligation trait: conservative by
//! construction (`evaluate` may return `PredicateVerdict::Refuse` too eagerly, never
//! `PredicateVerdict::Admit` too eagerly). `PredicateRegistry`/`undischarged_kinds` give the
//! "no silent vacuous pass" coverage check: every
//! `ConfigPredicate` `CharacteristicKind` must be named by at least one registered predicate's
//! `CapabilityPredicate::discharges`.
//!
//! # A worked example
//! `SimultaneousSubruleOverlapPredicate` implements the `simultaneous.subrule-overlap` predicate
//! via the REAL automaton intersection `crate::lower` provides — see that module's own
//! doc for how the intersection runs. Lowering happens inside `evaluate`, on demand, for the
//! subrule pairs that survive the gate early-outs.
//!
//! # `PlanNode` vs. `PlanNodeKind`
//! This module's trait takes `&PlanNodeKind`, not `&PlanNode`: `crate::plan` has no type literally
//! named `PlanNode` — its closed node-kind enum is `crate::plan::PlanNodeKind`, while a node's
//! *identity* is its separately-interned `crate::plan::NodeId`. Flagged here as a deliberate
//! naming divergence, not silently reconciled.
//!
//! # Bottom-up envelope composition + the CHECK-ONLY `CompileDecision`
//! `compose_envelope` runs `characterize` to get the profile, walks
//! `crate::enumerate::enumerate_default`'s reified `crate::plan::Plan` bottom-up (a node's
//! verdict is the meet of its children's verdicts and its own node-level predicate), and folds in
//! every observed non-`Proven` characteristic that has no plan-node-addressable predicate at all.
//! `meet` makes the lattice explicit (`Refuse` dominates `ConfirmOnly` dominates `Admit`);
//! `CompileDecision` widens `PredicateVerdict`'s single-diagnostic `Refuse` into a deduplicated
//! `Vec` so a caller sees every refusing construct in one pass. `compose_envelope` only computes
//! a decision — nothing here blocks or alters a compile path on it, and no interaction predicate
//! for `Union`/`Compose` nodes (via parallel-independence) exists in `default_registry`, so such
//! a node's "own predicate verdicts" are simply empty; see `compose_envelope`'s own doc for the
//! per-construct plan-node-mapping judgment calls.

use std::collections::{HashMap, HashSet};

use pg_grammar::model::{
    AffixAllomorphDef, AllomorphId, Dir, Grammar, MRuleId, MorphRuleDef, MorphRuleOrder,
    MprGroupMatchType, MprGroupOutput, MprSet, NatClassId, NaturalClassKind, OutputAction, PRuleId,
    PartRef, PhonRuleDef, ReduplicationHint, RewriteMode, StratumId,
};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::enumerate::EmissionStrategy;
use crate::grammar_semantics::GrammarSemantics;
use crate::plan::{FragmentSpec, NodeId, Plan, PlanNodeKind};
use crate::strategy_coverage::ALL_STRATEGIES;

// ---- Disposition + CharacteristicKind + the characterizer ----

/// A characteristic's capability disposition. Ordered here from "most trusted" to
/// "least" purely for reading convenience — no code relies on `Disposition`'s ordinal value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Disposition {
    /// Proven faithful; no predicate needed, admission-filtering allowed unconditionally.
    Proven,
    /// Compiles conditionally: `ConfirmOnly` unless/until a registered predicate proves `Admit`
    /// for the specific configuration observed.
    ConfigPredicate,
    /// Recall-preserving only if the proposer proposes the superset (no proven no-false-negative
    /// admission filter) — a first-class, non-failure verdict.
    ConfirmOnly,
}

/// The closed set of observed grammar characteristics, one variant per characteristic family.
/// Deliberately **not** one variant per individual `model.rs` enum *variant* in every case —
/// where several variants of one enum collapse into a single named characteristic (e.g.
/// `OutputAction`'s four variants all feed "output-action kind"), this enum mirrors that
/// collapse; `characterize`'s per-variant `match` arms still stay individually written (no
/// catch-all), so the exhaustiveness discipline holds at the `model.rs` level even where several
/// arms produce the same `CharacteristicKind`.
/// `Ord` is additive and carries no behavior: it is derived declaration order (the same order
/// `CharacteristicKind::ALL` lists), so a `BTreeSet<CharacteristicKind>` -- which is what
/// `crate::backend_mechanism::MechanismNode::construct_requirements` is -- iterates
/// deterministically. Nothing in the capability gate itself reads it.
///
/// Serde is hand-written over stable snake_case wire names, preserving the coverage-ledger format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CharacteristicKind {
    /// `MorphRuleDef::AffixProcess` (model.rs:543).
    Affixation,
    /// `MorphRuleDef::Realizational` (model.rs:546).
    RealizationalMorphology,
    /// `MorphRuleDef::Compounding` (model.rs:544).
    Compounding,
    /// `MorphRuleOrder::Linear` (model.rs:1058).
    OrderedMorphRuleApplication,
    /// `MorphRuleOrder::Unordered` (model.rs:1059).
    UnorderedMorphRuleApplication,
    /// `MprGroupOutput::Append` (model.rs:833).
    MprGroupAppend,
    /// `MprGroupOutput::Overwrite` (model.rs:832).
    MprGroupOverwrite,
    /// `RewriteMode::Iterative` (model.rs:386).
    IterativeRewrite,
    /// `RewriteMode::Simultaneous` (model.rs:387). Discharged by
    /// `SimultaneousSubruleOverlapPredicate`.
    SimultaneousRewrite,
    /// `Dir::LeftToRight` (model.rs:392).
    LeftToRightRewrite,
    /// `Dir::RightToLeft` (model.rs:393).
    RightToLeftRewrite,
    /// `PhonRuleDef::Metathesis` (model.rs:405).
    Metathesis,
    /// A `RewriteRuleDef` whose `lhs` pattern is empty (model.rs:417's own doc: "empty pattern if
    /// absent (epenthesis rules)") — an insertion-only rule.
    Epenthesis,
    /// A `RewriteSubruleDef` declaring a nontrivial `required_pos`/`required_mpr`/`excluded_mpr`
    /// (model.rs:423-427) — drives `gate.rs`'s partition, already Proven by that mechanism.
    SubruleGating,
    /// A "circumfix-shaped" `AffixAllomorphDef`: a multi-part LHS where the RHS's
    /// `OutputAction`s (model.rs:687) never `Copy` at least one LHS part — i.e. real subtracted/
    /// discontinuous material. NOT raised for every `OutputAction` occurrence (see
    /// `allomorph_drops_lhs_material`'s doc for why that would be unsound-by-over-triggering).
    CircumfixOutputAction,
    /// An `AffixAllomorphDef` whose RHS truly reduplicates: some `Input` part is echoed by
    /// `Copy`/`Modify` actions >= 2 times (model.rs:679's `ReduplicationHint`). NOT raised for
    /// every allomorph carrying a `ReduplicationHint` value (see `rhs_has_true_reduplication`'s
    /// doc — `Implicit` is the DTD default for every non-reduplicating affix too). Discharged by
    /// `ReduplicationPeelSupportedPredicate`: statically proven prefix-copy, suffix-copy, and
    /// one-separator layouts are peeled; every other true-reduplication shape uses structural
    /// synthesis instead.
    Reduplication,
    /// A `MorphemeCoOccurrenceRuleDef`/`AllomorphCoOccurrenceRuleDef` occurrence (model.rs:508's
    /// `CoOccurrenceAdjacency`, each variant folded into this one characteristic).
    CoOccurrenceConstraint,
    /// A `NaturalClassKind` variant (model.rs:361) — representational only, no capability
    /// implication either way; still matched exhaustively for the discipline.
    NaturalClassDefinition,
    /// `Grammar::char_tables.len() > 1` (model.rs:1100): more than one
    /// `CharacterDefinitionTable`, each stratum's own `StratumDef::table` (model.rs:1066)
    /// potentially disagreeing about what a raw segment index means. NOT one variant of an
    /// existing enum — a grammar-level configuration fact, discharged by
    /// `MultiTableFaithfulThreadingPredicate`. See that predicate's own doc for the
    /// admit/confirm-only/refuse split.
    MultiTable,
    /// A root allomorph entered on a non-final stratum whose bundles the final table spells
    /// differently than its own text does, before any rule runs
    /// (`crate::emit::cross_table_respelling`, the emitter's own computation): a segment is its
    /// feature bundle and a table only spells it for one stratum, so the root surfaces spelled by
    /// whichever final-table segment carries the same bundle (hc.dll
    /// `CharacterDefinitionTable.GetMatchingStrReps`). Distinct from `MultiTable`, which every
    /// two-table grammar raises, and from a shared representation, where both tables spell the
    /// root alike and only a later rule can change it.
    CrossTableRespelling,
    /// A `PatternNode::Quantifier` (`<OptionalSegmentSequence min max>`) occurrence anywhere in a
    /// `RewriteRuleDef`'s own LHS, or any of its subrules' RHS/left-env/right-env patterns. NOT
    /// one variant of `RewriteMode`/`Dir` (those already have their own characteristics) — a
    /// grammar-level structural fact about WHICH pattern nodes a rule's own patterns use,
    /// discharged by `QuantifierBoundedExpansionPredicate`. See that predicate's own doc for the
    /// compile-attempted split (bounded and unbounded both compile now; `all_bounded` is
    /// informational only, see `QuantifierPatternDetail`'s own doc).
    QuantifierPattern,
    /// `RootAllomorphDef::stem_name` (model.rs:798, `Option<StemNameId>`; `StemNameDef` at
    /// model.rs:816): a root allomorph restricted to a `<StemName>` region, checked only by
    /// `pg_rules::validity`'s `stem_name_gate_reason`/`stem_name_required_match` (C#
    /// `StemName.IsRequiredMatch`/`IsExcludedMatch`) at confirm time against the word's
    /// accumulated syntactic FS. Not represented by any `CharacteristicKind` until this one was
    /// added — a taxonomy gap one level more basic than an unbuilt filter (the compiler's
    /// construct ledger did not even record that stem names exist). `crate::emit` has no
    /// stem-name-aware admission filter anywhere (grep confirms zero references outside
    /// `precision.rs`'s own `ConstraintFamily::StemName` "Not populated" note); every
    /// stem-restricted root allomorph is proposed unconditionally, discharged only by confirm —
    /// hence `ConfirmOnly`, never anything stronger, until a real predicate is built.
    StemName,
    /// The disjunctive-allomorph re-check (`pg_rules::validity::allomorphs_valid_impl`,
    /// `free_fluctuates`/`disjunctive_candidates`/`root_constraints_equal`; C#
    /// `Allomorph.cs:127-152`): engaged whenever a `LexEntryDef` (model.rs:768) carries more than
    /// one `RootAllomorphDef` (model.rs:777, `allomorphs: Vec<RootAllomorphDef>`) — confirm then
    /// enforces "first-listed matching allomorph wins" for any two allomorphs whose own
    /// `environments`/`is_bound` (model.rs:791-792) do NOT compare equal (`root_constraints_equal`;
    /// when they DO compare equal, the allomorphs "free-fluctuate" and either is accepted). Not a
    /// distinct `model.rs` enum variant — a derived cross-allomorph relation, so, like `StemName`,
    /// missing from this ledger until now. `crate::emit` builds no ordering/preference machinery
    /// for this at all (every allomorph of a multi-allomorph entry is proposed uniformly, in every
    /// position); the emitter's own bare-root discharge (`RootRec::never_valid_bare`) deliberately
    /// restricts itself to the entry-has-exactly-one-allomorph case specifically to avoid needing
    /// to reason about this mechanism at all — so this characteristic remains wholly
    /// `ConfirmOnly`, undischarged by anything this crate compiles.
    FreeFluctuation,
    /// A `Modify`-only allomorph: the input is mutated in place rather than affixed to (ablaut,
    /// mutation, simulfix). Distinct from `CircumfixOutputAction`, which cannot fire here at all --
    /// its `allomorph_drops_lhs_material` trigger returns early on a single-part input, and a
    /// single-part input is exactly the ablaut shape.
    ProcessMorphology,
}

impl CharacteristicKind {
    /// Every `CharacteristicKind` variant — hand-maintained (Rust has no enum reflection), so
    /// adding a variant above and forgetting to add it here is a real gap `undischarged_kinds`
    /// cannot see. `crate::capability::tests::all_kinds_have_a_default_disposition` is the
    /// closest available backstop (it re-derives disposition via `Self::default_disposition`,
    /// which itself IS exhaustively matched — a variant missing from `ALL` would simply never be
    /// checked, not panic, so this is a documented gap, not a proven-closed one).
    pub const ALL: &'static [CharacteristicKind] = &[
        CharacteristicKind::Affixation,
        CharacteristicKind::RealizationalMorphology,
        CharacteristicKind::Compounding,
        CharacteristicKind::OrderedMorphRuleApplication,
        CharacteristicKind::UnorderedMorphRuleApplication,
        CharacteristicKind::MprGroupAppend,
        CharacteristicKind::MprGroupOverwrite,
        CharacteristicKind::IterativeRewrite,
        CharacteristicKind::SimultaneousRewrite,
        CharacteristicKind::LeftToRightRewrite,
        CharacteristicKind::RightToLeftRewrite,
        CharacteristicKind::Metathesis,
        CharacteristicKind::Epenthesis,
        CharacteristicKind::SubruleGating,
        CharacteristicKind::CircumfixOutputAction,
        CharacteristicKind::Reduplication,
        CharacteristicKind::CoOccurrenceConstraint,
        CharacteristicKind::NaturalClassDefinition,
        CharacteristicKind::MultiTable,
        CharacteristicKind::CrossTableRespelling,
        CharacteristicKind::QuantifierPattern,
        CharacteristicKind::StemName,
        CharacteristicKind::FreeFluctuation,
        CharacteristicKind::ProcessMorphology,
    ];

    /// The characteristic's disposition BEFORE any predicate runs. Exhaustively matched (no
    /// catch-all) — adding a `CharacteristicKind` variant breaks this build too, same discipline
    /// as `characterize`'s own `model.rs` matches.
    pub fn default_disposition(self) -> Disposition {
        match self {
            CharacteristicKind::Affixation => Disposition::Proven,
            // Grammar-global, so it stays ConfirmOnly regardless of any one strategy's structural refusal; the two answer different questions and both hold at once.
            CharacteristicKind::RealizationalMorphology => Disposition::ConfirmOnly,
            // Faithful, depth-budgeted proposal exists but no admission-filter proof; see `CompoundingRecursionSafePredicate`'s own doc.
            CharacteristicKind::Compounding => Disposition::ConfigPredicate,
            CharacteristicKind::OrderedMorphRuleApplication => Disposition::Proven,
            // Already an ordering-union proposal but unproven; see `UnorderedOrderingUnionPredicate`'s own doc.
            CharacteristicKind::UnorderedMorphRuleApplication => Disposition::ConfigPredicate,
            CharacteristicKind::MprGroupAppend => Disposition::ConfirmOnly,
            CharacteristicKind::MprGroupOverwrite => Disposition::ConfigPredicate,
            CharacteristicKind::IterativeRewrite => Disposition::Proven,
            CharacteristicKind::SimultaneousRewrite => Disposition::ConfigPredicate,
            CharacteristicKind::LeftToRightRewrite => Disposition::Proven,
            // Faithful via `compile_rtl_branch_net` but unproven; see `RightToLeftRewriteFaithfulReversalPredicate`'s own doc.
            CharacteristicKind::RightToLeftRewrite => Disposition::ConfigPredicate,
            // Faithful via `compile_metathesis_rule` but unproven, pinned by `metathesis_predicate_confirm_only_for_right_to_left_rule`.
            CharacteristicKind::Metathesis => Disposition::ConfigPredicate,
            CharacteristicKind::Epenthesis => Disposition::ConfigPredicate,
            CharacteristicKind::SubruleGating => Disposition::Proven,
            CharacteristicKind::CircumfixOutputAction => Disposition::ConfigPredicate,
            // Faithfully proposed by either the shared peeler or structural synthesis, but unproven; see `ReduplicationPeelSupportedPredicate`.
            CharacteristicKind::Reduplication => Disposition::ConfigPredicate,
            CharacteristicKind::CoOccurrenceConstraint => Disposition::ConfirmOnly,
            CharacteristicKind::NaturalClassDefinition => Disposition::Proven,
            // Threads each rule's own owning table faithfully but unproven; see `MultiTableFaithfulThreadingPredicate`'s own doc.
            CharacteristicKind::MultiTable => Disposition::ConfigPredicate,
            // The mainline proposes the respelled surface as one more variant and confirm keeps or prunes it; no admission-filter proof exists.
            CharacteristicKind::CrossTableRespelling => Disposition::ConfirmOnly,
            // Bounded or unbounded alpha-free quantifiers compile faithfully but unproven; see `QuantifierBoundedExpansionPredicate`'s own doc.
            CharacteristicKind::QuantifierPattern => Disposition::ConfigPredicate,
            // `crate::emit` has no stem-name-aware admission filter; `pg_rules::validity::stem_name_gate_reason` discharges it only at confirm time.
            CharacteristicKind::StemName => Disposition::ConfirmOnly,
            // Same shape: `crate::emit` proposes every allomorph uniformly; confirm enforces "first-listed wins".
            CharacteristicKind::FreeFluctuation => Disposition::ConfirmOnly,
            // Correct via replayed real synthesis, unproven as an admission filter; cost is health's.
            CharacteristicKind::ProcessMorphology => Disposition::ConfirmOnly,
        }
    }
}

/// Which `model.rs` construct occurrence induced a `CharacteristicObservation` — each
/// observation is tagged with the model location(s) that induced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelLocation {
    MorphRule(MRuleId),
    /// One allomorph of an `AffixProcess`/`Realizational` rule (`MorphRuleDef::affix_allomorphs`).
    AffixAllomorph {
        rule: MRuleId,
        allomorph_index: usize,
    },
    Stratum(StratumId),
    /// Index into `Grammar::mpr_groups`.
    MprGroup(usize),
    PhonRule(PRuleId),
    RewriteSubrule {
        rule: PRuleId,
        subrule_index: usize,
    },
    NaturalClass(NatClassId),
    /// Index into `Grammar::morphemes` whose `co_occurrence` list this observation came from.
    MorphemeCoOccurrence(usize),
    AllomorphCoOccurrence(AllomorphId),
    /// A `RootAllomorphDef` (`pg_grammar::model::LexEntryDef::allomorphs`) whose own fields
    /// induced the observation directly -- `StemName` (its `stem_name` is `Some`),
    /// `FreeFluctuation` (it compares `root_constraints_equal` to a sibling allomorph of the same
    /// entry), or `CrossTableRespelling` (the final table spells it differently than its own).
    /// Distinct from `AllomorphCoOccurrence`: that variant is keyed by a co-occurrence
    /// *rule* attached to the allomorph, not a property of the allomorph itself.
    RootAllomorph(AllomorphId),
}

/// Per-subrule gate/opacity facts one simultaneous rule carries, and the whole of what
/// `SimultaneousSubruleOverlapPredicate` reads from the profile.
///
/// Gate flags are facts about the grammar and belong in a characteristics projection. A
/// lowered span is a compiled artifact and does not: the predicate lowers what it needs, when
/// it needs it, from the grammar it is now handed. See `CONTEXT.md`, Artifact and Lowering.
#[derive(Debug, Clone, Copy)]
pub struct SubruleGateInfo {
    pub index: usize,
    pub required_mpr: MprSet,
    pub excluded_mpr: MprSet,
    pub self_opaquing: bool,
}

/// `ObservationDetail::SimultaneousRewrite`'s payload: one rule's full subrule-gate table.
#[derive(Debug, Clone)]
pub struct SimultaneousRewriteDetail {
    pub rule: PRuleId,
    pub subrules: Vec<SubruleGateInfo>,
}

/// `ObservationDetail::MultiTable`'s payload: the structural fact
/// `MultiTableFaithfulThreadingPredicate` needs, computed once here rather than re-derived at
/// `evaluate` time: this profile is a self-contained projection of grammar FACTS
/// (`CONTEXT.md`, Lowering — a lowered span is an artifact and does not belong here).
#[derive(Debug, Clone)]
pub struct MultiTableDetail {
    /// `g.char_tables.len()`.
    pub table_count: usize,
    /// `true` iff NO two distinct tables share a normalized representation (spelling) — the
    /// structural condition `MultiTableFaithfulThreadingPredicate`'s own doc explains: per-rule
    /// table-correct resolution (this change's `pg_foma::replace::owning_table` fix) is faithful
    /// with no residual cross-table token-collision risk exactly when every table's own character
    /// inventory is disjoint from every other's.
    pub representations_pairwise_disjoint: bool,
    /// The first shared representation found (any two tables, document order), if
    /// `representations_pairwise_disjoint` is `false` — a concrete witness for the diagnostic,
    /// never just "some tables overlap somewhere".
    pub shared_representation_witness: Option<String>,
}

/// `ObservationDetail::RightToLeftRewrite`'s payload: whether
/// `crate::replace::compile_rtl_branch_net`'s reversal construction can even be
/// ATTEMPTED for this specific `Dir::RightToLeft` rule — computed once here, a grammar fact in a
/// self-contained projection, by re-running the SAME structural
/// pattern-shape check `crate::replace::compile_rewrite_rule_subset` itself gates on: every
/// LHS/RHS/environment pattern must avoid a malformed `Quantifier`
/// (non-inverted and non-empty, alpha-free in its own children; a genuinely UNBOUNDED quantifier,
/// `max=-1`, is no longer by itself
/// disqualifying), and `Segments`/`Anchor` no longer disqualify EITHER, provided any `Segments`
/// node shares the rule's own owning table (`crate::lower::PatternLowerScope::RewriteRuleCompile`'s
/// own doc has the full, current exclusion list) — via `crate::replace::pattern_slots`/
/// `crate::replace::owning_table` directly, WITHOUT compiling any foma automaton (cheap, purely
/// structural, no `FomaOptions`/`SegAlphabet` needed). `Simultaneous` mode is handled by its own
/// `CharacteristicKind::SimultaneousRewrite` observation, whose own admitted set this detail
/// computation leaves untouched, so this
/// detail is only ever computed for `Dir::RightToLeft` rules (`characterize`'s
/// own `Dir::RightToLeft` arm) — a rule that is BOTH `Simultaneous` and `RightToLeft` gets both
/// observations, and `RightToLeftRewriteFaithfulReversalPredicate`'s own verdict is irrelevant
/// there since `SimultaneousRewrite`'s own predicate verdict already dominates under `meet`.
#[derive(Debug, Clone, Copy)]
pub struct RightToLeftRewriteDetail {
    pub rule: PRuleId,
    /// `true` iff every LHS/RHS/environment pattern in this rule's subrules is a shape
    /// `crate::replace::pattern_slots` accepts under `PatternLowerScope::RewriteRuleCompile` (no
    /// malformed `Quantifier`, no cross-table `Segments` -- see this
    /// struct's own top doc for exactly which shapes that excludes) AND the rule resolves to a real
    /// owning `pg_grammar::chardef::CharDefTable` — i.e. exactly the construct-shape floor
    /// `compile_rewrite_rule_subset` itself requires before it ever calls [`fsm_reverse`
    /// ](foma::reverse::fsm_reverse). `false` means the rule is STILL honestly skipped
    /// (`None`) by the real compiler, same as any other unsupported pattern construct.
    pub reversal_construction_attempted: bool,
    /// The SPECIFIC reason `reversal_construction_attempted` is `false`, if it IS `false` and the
    /// reason is a pattern-shape one — `None` when `reversal_construction_attempted` is `true`
    /// (nothing to diagnose), or when the rule has no resolvable owning table at all (a
    /// non-pattern-shape reason `crate::lower::UnsupportedPatternNode` has no variant for).
    /// Names the specific shape rather than a generic "unsupported pattern" —
    /// `RightToLeftRewriteFaithfulReversalPredicate::evaluate` reads this to build a precise
    /// `Refuse` witness instead of a laundry-list "could be any of these" message.
    pub unsupported_reason: Option<crate::lower::UnsupportedPatternNode>,
}

/// `ObservationDetail::QuantifierPattern`'s payload: the two independent facts
/// `QuantifierBoundedExpansionPredicate` needs about a rule observed to use
/// `PatternNode::Quantifier` somewhere in its own LHS/RHS/environment patterns.
/// `ObservationDetail::Metathesis`'s payload: the
/// one structural fact `MetathesisFaithfulSwapPredicate` needs about a `PhonRuleDef::Metathesis`
/// rule, computed once here as a grammar fact in a self-contained projection rather than
/// re-derived at `evaluate` time.
#[derive(Debug, Clone, Copy)]
pub struct MetathesisDetail {
    pub rule: PRuleId,
    /// `true` iff `crate::replace::compile_metathesis_rule`'s own structural admission floor is
    /// met: a resolvable owning table (`crate::replace::owning_table_for_metathesis`),
    /// `left_switch != right_switch` both in bounds, and the WHOLE pattern is a shape
    /// `crate::replace::pattern_slots` accepts with no `crate::replace::Slot::Alpha`/
    /// `crate::replace::Slot::Repeat` occurrence anywhere.
    ///
    /// **Dir-agnostic**: this field no longer gates on
    /// `Dir::LeftToRight` at all -- `crate::replace::compile_metathesis_rule` now compiles
    /// `Dir::RightToLeft` too, via the SAME mirror-and-reverse construction
    /// `compile_rtl_branch_net` already uses for RTL rewrite rules (that function's own module
    /// doc, "`Dir::RightToLeft`" section), so the structural admission floor is identical for
    /// either direction -- mirrors `RightToLeftRewriteDetail::reversal_construction_attempted`'s
    /// own already-Dir-agnostic convention (that field characterizes pattern-shape support
    /// independent of `rule.dir` too).
    ///
    /// A `Slot::Alpha` occurrence is genuinely STRUCTURALLY IMPOSSIBLE for a `<MetathesisRule>`
    /// (not merely unattested): `pg_grammar::load::load_metathesis_rule` resolves every pattern
    /// node against an EMPTY `VarTable::default()` (no `<Variables>` scope exists for a
    /// `<MetathesisRule>` at all), so any `<AlphaVariable>` inside one errors the WHOLE grammar
    /// load before a `Slot::Alpha` could ever be produced -- see `crate::replace::
    /// compile_metathesis_rule`'s own module doc for the full citation. A `Slot::Repeat`
    /// occurrence, by contrast, IS structurally reachable (`OptionalSegmentSequence` is DTD-legal
    /// inside a `<MetathesisRule>`'s own `<PhoneticSequence>`, just never attested in any fixture
    /// this crate has authored) -- refused regardless of `Dir` by `crate::replace`'s own
    /// `slot_candidates`, so this stays an honest, reachable (not vacuous) scope line for either
    /// direction.
    pub swap_construction_attempted: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct QuantifierPatternDetail {
    pub rule: PRuleId,
    /// `true` iff EVERY `Quantifier` occurrence anywhere in this rule's patterns (LHS, every
    /// subrule's RHS/left-env/right-env, at ANY nesting depth) has a concrete `max` bound
    /// (`rule_has_unbounded_quantifier`'s own negation) — `false` means at least one is genuinely
    /// unbounded (`max == None`, the DTD's `max="-1"` Kleene sentinel).
    ///
    /// **Informational only**:
    /// `QuantifierBoundedExpansionPredicate` no longer branches on this field at all (a genuinely
    /// unbounded quantifier compiles via the SAME `crate::replace::Slot::Repeat` construction a
    /// bounded one does, `compile_attempted` below is the only fact that matters for disposition
    /// now) — `all_bounded` is retained purely as structural evidence for OTHER consumers,
    /// specifically `crate::characterization`'s own per-rule cost-uncertainty health finding (an unbounded
    /// quantifier's own FST-compile cost is not characterization-boundable ahead of time, a `LargeMultiplier`-level
    /// observation independent of whether the grammar's capability gate admits the rule).
    pub all_bounded: bool,
    /// `true` iff `rtl_reversal_construction_attempted` accepts this rule's WHOLE pattern shape
    /// (every LHS/RHS/environment pattern is `crate::replace::pattern_slots`-acceptable and the
    /// rule resolves to a real owning table) — reused verbatim from the RTL predicate's own
    /// structural probe (that function's own doc: it is Dir-agnostic, a generic "is this rule's
    /// pattern shape compilable at all" check), not re-derived, since it is EXACTLY the question
    /// this detail also needs: even a rule whose every quantifier is individually bounded can still
    /// be blocked from compiling by some OTHER unsupported construct in the SAME rule (an ambiguous
    /// disagree-polarity alpha var, a malformed `Quantifier` elsewhere in its patterns, or an
    /// unresolvable owning table) — `false` in that case, so the predicate never claims more than the real compiler
    /// actually attempts.
    pub compile_attempted: bool,
}

/// `ObservationDetail::CircumfixOutputAction`'s payload: the one structural fact
/// `CircumfixStructuralCompositePredicate` needs about an `AffixAllomorphDef` whose RHS drops
/// real LHS material (`allomorph_drops_lhs_material`'s own trigger — circumfix wrapping, a
/// null-role subtractive input, or any other "real subtracted/discontinuous material" shape that
/// function's own doc names), computed once here as a grammar fact in a self-contained
/// projection rather than re-derived at `evaluate` time.
#[derive(Debug, Clone, Copy)]
pub struct CircumfixOutputActionDetail {
    pub rule: MRuleId,
    pub allomorph_index: usize,
    /// `true` iff `crate::emit::is_structural_rule` routes THIS observation's owning rule through
    /// `crate::emit`'s `build_structural_composites` — the mechanism that resynthesizes every
    /// candidate surface via the REAL morphological engine (`pg_rules::morph::synthesize`) rather
    /// than splicing literal `InsertSegments` text, and so is faithful (never a silent wrong
    /// compile) for whatever shape a rule routed there actually has, `OutputAction` variant
    /// notwithstanding. `is_structural_rule` is per-RULE: it admits a rule whenever ANY
    /// of its allomorphs classifies `Role::CircumfixPrefix` (not only allomorph 0 — `rule_role`'s
    /// own first-allomorph contract is unchanged and unrelated; `is_structural_rule` asks its own
    /// allomorph-wise question via a dedicated helper), so a rule with a mix of plain and
    /// circumfix-shaped allomorphs is admitted as soon as ONE allomorph qualifies. Every allomorph of
    /// a covered rule still shares this same `true`/`false` value (computed once per allomorph
    /// anyway, not memoized across allomorphs of the same rule, to keep this detail self-contained
    /// per observation, mirroring `MetathesisDetail`/`RightToLeftRewriteDetail`'s own "cheap,
    /// recompute don't share" convention) — because `build_structural_composites` synthesizes the
    /// WHOLE rule's surface via `pg_rules::morph::synthesize`, which does not special-case by
    /// allomorph, once the rule is admitted every allomorph rides along.
    ///
    /// `false` means NO allomorph of this rule is `CircumfixPrefix`, none carries an
    /// `OutputAction::Modify`/`InsertContext`, and allomorph 0's role (per
    /// `crate::emit::classify_affix`) is `Role::Reduplication`/`Role::CircumfixSuffix` — since
    /// census C4, `None`/`Prefix`/`Suffix`/`Infix` all route through `is_structural_rule`'s
    /// drop-aware arm, and that arm scans EVERY allomorph of the rule
    /// (`.any(rhs_drops_lhs_material)`, not only allomorph 0), so it is covered whenever THIS
    /// observation's own dropping allomorph exists, regardless of which of those four roles
    /// allomorph 0 itself carries. E.g. a rule whose primary shape is genuine reduplication
    /// (`crate::peel::ReduplicationPeeler`'s own job) with a distinct dropping allomorph elsewhere.
    /// `Role::Process` is NOT such a case: `is_structural_rule` admits it unconditionally, because
    /// the structural composite replays `pg_rules::morph::synthesize` rather than emitting literal
    /// text. The real compiler already honestly skips a `false` allomorph everywhere (never silently
    /// mis-compiled): `crate::emit::emit_rule_allomorphs`'s own role/zone check reports it
    /// `uncovered`, and it never reaches `build_structural_composites` either.
    pub structural_composite_attempted: bool,
}

/// `ObservationDetail::Reduplication`'s payload: the actual proposal-route facts for an
/// `AffixAllomorphDef` whose RHS truly reduplicates. The informational rule-kind bit preserves the
/// original model distinction, while the two dispositive route bits are read from the same shared
/// predicates used by the runtime peeler and structural emitter.
#[derive(Debug, Clone, Copy)]
pub struct ReduplicationDetail {
    pub rule: MRuleId,
    pub allomorph_index: usize,
    /// `true` iff `rule`'s owning `MorphRuleDef` is `MorphRuleDef::AffixProcess` — the only rule
    /// kind `crate::peel::ReduplicationPeeler` ever peels. `false` means this true-reduplicating
    /// allomorph belongs to a `MorphRuleDef::Realizational` rule: the peeler will never propose it
    /// (a documented, intentional C#-faithful non-support, not a bug to fix — see this struct's
    /// own doc and `crate::peel::is_reduplication_rule`'s doc for the citation).
    pub peel_eligible_rule_kind: bool,
    /// `true` iff the runtime peeler actually owns the whole rule. This is stricter than the XML
    /// rule-kind check: one structurally routed reduplicating alternative makes the peeler
    /// relinquish every alternative in that mixed rule.
    pub peel_attempted: bool,
    /// `true` iff structural synthesis owns the whole rule and replays the full morphology engine.
    pub structural_composite_attempted: bool,
}

/// `ObservationDetail::Compounding`'s payload: the one structural fact
/// `CompoundingRecursionSafePredicate` needs about a
/// `MorphRuleDef::Compounding` occurrence — whether `compounding_recursive` (this module's own
/// grammar-rule-graph reachability pass — a kind of predicate input beyond the per-rule/per-subrule
/// checks other predicates in this file use) proved this
/// specific rule's head/non-head stem search can be reached by another `Compounding` application's
/// own output.
#[derive(Debug, Clone, Copy)]
pub struct CompoundingDetail {
    pub rule: MRuleId,
    /// `true` iff `rule` is `compounding.recursive` — self-feeding (`rule.max_apps() > 1`) or
    /// reachable from a
    /// DISTINCT `Compounding` rule sharing or preceding its stratum (`compounding_recursive`'s own
    /// doc for the exact, deliberately conservative reachability test). `false` means
    /// `compounding.non-recursive` — the license-gated propose shape (`crate::emit::
    /// compound_license`) applies and `CompoundingRecursionSafePredicate` returns `ConfirmOnly`.
    pub recursive: bool,
    /// Turning a boolean into a bound: `compounding_max_depth`'s own finite upper bound on the
    /// number of STEMS (lexical roots) any single compounding derivation chain ending in an
    /// application of `rule` could combine. `2` is the ordinary head+non-head shape `compounding.
    /// non-recursive` already covers faithfully; `recursive == (max_depth > 2)` always holds (see
    /// `compounding_max_depth`'s own doc for the equivalence argument) — this field is strictly
    /// MORE informative than `recursive`, never in tension with it. See `compounding_max_depth`'s
    /// own doc for the bound's derivation and for why it is ALWAYS finite for this construct (no
    /// "genuinely unboundable" shape exists, unlike `CharacteristicKind::QuantifierPattern`'s real
    /// `max == -1` Kleene case).
    pub max_depth: usize,
}

/// `ObservationDetail::UnorderedStratum`'s payload: the one cardinality fact
/// `UnorderedOrderingUnionPredicate` needs about a `StratumDef` declaring
/// `MorphRuleOrder::Unordered` — its own loose-rule count, computed in
/// `crate::unordered::stratum_metrics` rather than re-derived here.
#[derive(Debug, Clone, Copy)]
pub struct UnorderedStratumDetail {
    pub stratum: StratumId,
    /// This stratum's own `sd.mrules.len()` — the quantity `crate::emit::build_deriv_chain`'s own
    /// `depth` for a role zone equals (that function's own doc), and so the quantity whose growth
    /// predicts this construction's compiled-network cost (`crate::unordered`'s own module doc,
    /// "Big-O").
    pub rule_count: usize,
}

/// `ObservationDetail::CrossTableRespelling`'s payload: one inner-stratum root and the two spellings it has, its own and the final table's.
#[derive(Debug, Clone)]
pub struct CrossTableRespellingDetail {
    pub allomorph: AllomorphId,
    pub own_spelling: String,
    pub surface_spelling: String,
}

/// Extra structured data an observation needs beyond `kind`/`disposition`/`location`, for the
/// characteristics that a predicate must inspect at finer grain than "did this occur at all".
/// Most characteristics carry `None` — [`CharacteristicKind::
/// SimultaneousRewrite`] needs `Self::SimultaneousRewrite`,
/// `CharacteristicKind::MultiTable` needs `Self::MultiTable`,
/// `CharacteristicKind::RightToLeftRewrite` needs
/// `Self::RightToLeftRewrite`,
/// `CharacteristicKind::CircumfixOutputAction` needs `Self::CircumfixOutputAction`,
/// and `CharacteristicKind::Reduplication` needs
/// `Self::Reduplication`.
#[derive(Debug, Clone)]
pub enum ObservationDetail {
    None,
    SimultaneousRewrite(SimultaneousRewriteDetail),
    MultiTable(MultiTableDetail),
    CrossTableRespelling(CrossTableRespellingDetail),
    RightToLeftRewrite(RightToLeftRewriteDetail),
    QuantifierPattern(QuantifierPatternDetail),
    Metathesis(MetathesisDetail),
    CircumfixOutputAction(CircumfixOutputActionDetail),
    Reduplication(ReduplicationDetail),
    Compounding(CompoundingDetail),
    UnorderedStratum(UnorderedStratumDetail),
}

/// One occurrence of a characteristic in a `CharacteristicsProfile`.
#[derive(Debug, Clone)]
pub struct CharacteristicObservation {
    pub kind: CharacteristicKind,
    pub disposition: Disposition,
    pub location: ModelLocation,
    pub detail: ObservationDetail,
}

impl CharacteristicObservation {
    /// `disposition` is always derived from `kind`, enforced structurally rather than by convention at each call site.
    fn new(kind: CharacteristicKind, location: ModelLocation, detail: ObservationDetail) -> Self {
        CharacteristicObservation {
            disposition: kind.default_disposition(),
            kind,
            location,
            detail,
        }
    }
}

/// Cheap grammar-scale facts fed to cost/planning, not the correctness gate itself.
#[derive(Debug, Clone, Copy, Default)]
pub struct GrammarCardinality {
    pub entry_count: usize,
    pub morpheme_count: usize,
    pub mrule_count: usize,
    pub prule_count: usize,
    pub stratum_count: usize,
    /// Max reachable derivation chain depth (e.g. the Aweti 24-level chain), left `None` rather
    /// than guessed when not cheaply computable: computing it for real needs the morphotactic
    /// reachability automaton (`crate::morphotactics`/`pg_rules::stratum`), a genuine per-grammar
    /// graph analysis rather than a field lookup.
    pub max_derivation_chain_depth: Option<usize>,
}

/// Every observed characteristic plus the grammar's cardinality facts.
#[derive(Debug, Clone, Default)]
pub struct CharacteristicsProfile {
    observations: Vec<CharacteristicObservation>,
    pub cardinality: GrammarCardinality,
}

impl CharacteristicsProfile {
    pub fn observations(&self) -> &[CharacteristicObservation] {
        &self.observations
    }

    /// `true` iff at least one observation carries `disposition`.
    pub fn has_disposition(&self, disposition: Disposition) -> bool {
        self.observations
            .iter()
            .any(|o| o.disposition == disposition)
    }

    /// The `SimultaneousRewriteDetail` for phonological rule `rule`, if `rule` was observed as a
    /// `Simultaneous`-mode rewrite rule (`SimultaneousSubruleOverlapPredicate`'s own lookup).
    pub fn simultaneous_detail(&self, rule: PRuleId) -> Option<&SimultaneousRewriteDetail> {
        self.observations.iter().find_map(|o| match &o.detail {
            ObservationDetail::SimultaneousRewrite(d) if d.rule == rule => Some(d),
            _ => None,
        })
    }

    /// The grammar-wide `MultiTableDetail`, if `g.char_tables.len() > 1` was observed at all
    /// (`MultiTableFaithfulThreadingPredicate`'s own lookup).
    pub fn multi_table_detail(&self) -> Option<&MultiTableDetail> {
        self.observations.iter().find_map(|o| match &o.detail {
            ObservationDetail::MultiTable(d) => Some(d),
            _ => None,
        })
    }

    /// Every inner-stratum root the final table respells, one per `CrossTableRespelling` observation.
    pub fn cross_table_respelling_details(
        &self,
    ) -> impl Iterator<Item = &CrossTableRespellingDetail> {
        self.observations.iter().filter_map(|o| match &o.detail {
            ObservationDetail::CrossTableRespelling(d) => Some(d),
            _ => None,
        })
    }

    /// `rule`'s own `RightToLeftRewriteDetail`, if it was observed as a `Dir::RightToLeft` rule
    /// at all (`characterize`'s own `Dir::RightToLeft` arm).
    pub fn right_to_left_detail(&self, rule: PRuleId) -> Option<&RightToLeftRewriteDetail> {
        self.observations.iter().find_map(|o| match &o.detail {
            ObservationDetail::RightToLeftRewrite(d) if d.rule == rule => Some(d),
            _ => None,
        })
    }

    /// `rule`'s own `QuantifierPatternDetail`, if it was observed to use `PatternNode::Quantifier`
    /// anywhere in its own patterns at all (`characterize`'s own quantifier-scan block).
    pub fn quantifier_detail(&self, rule: PRuleId) -> Option<&QuantifierPatternDetail> {
        self.observations.iter().find_map(|o| match &o.detail {
            ObservationDetail::QuantifierPattern(d) if d.rule == rule => Some(d),
            _ => None,
        })
    }

    /// `rule`'s own `MetathesisDetail`, if it was observed as a `PhonRuleDef::Metathesis` rule at
    /// all (`characterize`'s own `PhonRuleDef::Metathesis` arm).
    pub fn metathesis_detail(&self, rule: PRuleId) -> Option<&MetathesisDetail> {
        self.observations.iter().find_map(|o| match &o.detail {
            ObservationDetail::Metathesis(d) if d.rule == rule => Some(d),
            _ => None,
        })
    }

    /// Every `CircumfixOutputActionDetail` observed at all (`characterize_allomorph`'s own
    /// `allomorph_drops_lhs_material` trigger) — plural, unlike the other `*_detail` lookups above:
    /// `CircumfixStructuralCompositePredicate` has no per-node address to key a single lookup on
    /// (this characteristic has no corresponding `crate::plan::PlanNodeKind` at all, same
    /// "grammar-wide, not node-specific" shape `MultiTableFaithfulThreadingPredicate`'s own doc
    /// describes), so it scans every observation itself rather than looking one up by id.
    pub fn circumfix_output_action_details(
        &self,
    ) -> impl Iterator<Item = &CircumfixOutputActionDetail> {
        self.observations.iter().filter_map(|o| match &o.detail {
            ObservationDetail::CircumfixOutputAction(d) => Some(d),
            _ => None,
        })
    }

    /// Every `ReduplicationDetail` observed at all (`characterize_allomorph`'s own
    /// `rhs_has_true_reduplication` trigger) — plural, like
    /// `Self::circumfix_output_action_details`: `Reduplication` has no corresponding
    /// `crate::plan::PlanNodeKind` either (peeling happens entirely outside the compiled FST, so
    /// there is no plan node to address it by), so `ReduplicationPeelSupportedPredicate` scans
    /// every observation itself rather than looking one up by id.
    pub fn reduplication_details(&self) -> impl Iterator<Item = &ReduplicationDetail> {
        self.observations.iter().filter_map(|o| match &o.detail {
            ObservationDetail::Reduplication(d) => Some(d),
            _ => None,
        })
    }

    /// Every `CompoundingDetail` observed at all — plural, same "no corresponding
    /// `crate::plan::PlanNodeKind`" shape
    /// `Self::reduplication_details`/`Self::circumfix_output_action_details` already use:
    /// `CompoundingRecursionSafePredicate` scans every observation itself rather than looking one
    /// up by a specific plan node.
    pub fn compounding_details(&self) -> impl Iterator<Item = &CompoundingDetail> {
        self.observations.iter().filter_map(|o| match &o.detail {
            ObservationDetail::Compounding(d) => Some(d),
            _ => None,
        })
    }

    /// Every `UnorderedStratumDetail` observed at all — plural, same "no corresponding
    /// `crate::plan::PlanNodeKind`" shape `Self::compounding_details`/
    /// `Self::reduplication_details` already use: `UnorderedOrderingUnionPredicate` scans every
    /// observation itself rather than looking one up by a specific plan node (`Unordered`'s
    /// ordering-union proposal is realized by `crate::emit::build_deriv_chain`'s existing
    /// derivation-layer construction, which has no reified `Plan` node of its own either).
    pub fn unordered_stratum_details(&self) -> impl Iterator<Item = &UnorderedStratumDetail> {
        self.observations.iter().filter_map(|o| match &o.detail {
            ObservationDetail::UnorderedStratum(d) => Some(d),
            _ => None,
        })
    }
}

// ---- Private per-construct characterization helpers ----

/// Re-runs `compile_rewrite_rule_subset`'s own admission floor structurally, `Dir`-agnostic.
/// See docs/research/pg-foma-capability-design-notes.md.
fn rtl_reversal_construction_attempted(g: &Grammar, r: &pg_grammar::model::RewriteRuleDef) -> bool {
    rtl_reversal_diagnosis(g, r).is_ok()
}

/// Names the specific failing shape, checked in the same order the real compiler checks it in.
/// See docs/research/pg-foma-capability-design-notes.md.
fn rtl_reversal_diagnosis(
    g: &Grammar,
    r: &pg_grammar::model::RewriteRuleDef,
) -> Result<(), Option<crate::lower::UnsupportedPatternNode>> {
    use crate::lower::{diagnose_unsupported, PatternLowerScope};
    let scope = PatternLowerScope::RewriteRuleCompile;
    let Some(table) = crate::replace::owning_table(g, r) else {
        return Err(None);
    };
    for sr in &r.subrules {
        // Mirrors `compile_rewrite_rule_subset`'s own loop: a fresh occurrence counter per subrule.
        let mut next_occurrence = 0usize;
        if crate::replace::pattern_slots(g, table, &r.lhs, &mut next_occurrence, scope).is_none() {
            return Err(Some(diagnose_unsupported(g, table, &r.lhs, scope)));
        }
        if crate::replace::pattern_slots(g, table, &sr.rhs, &mut next_occurrence, scope).is_none() {
            return Err(Some(diagnose_unsupported(g, table, &sr.rhs, scope)));
        }
        if let Some(p) = &sr.left_env {
            if crate::replace::pattern_slots(g, table, p, &mut next_occurrence, scope).is_none() {
                return Err(Some(diagnose_unsupported(g, table, p, scope)));
            }
        }
        if let Some(p) = &sr.right_env {
            if crate::replace::pattern_slots(g, table, p, &mut next_occurrence, scope).is_none() {
                return Err(Some(diagnose_unsupported(g, table, p, scope)));
            }
        }
    }
    Ok(())
}

/// Re-runs `compile_metathesis_rule`'s own structural admission floor, genuinely `Dir`-agnostic now.
/// See docs/research/pg-foma-capability-design-notes.md.
fn metathesis_swap_construction_attempted(
    g: &Grammar,
    m: &pg_grammar::model::MetathesisRuleDef,
) -> bool {
    let Some(table) = crate::replace::owning_table_for_metathesis(g, m) else {
        return false;
    };
    if m.left_switch == m.right_switch {
        return false;
    }
    let mut next_occurrence = 0usize;
    // Must stay the identical scope `compile_metathesis_rule` passes; see this fn's own doc.
    let scope = crate::lower::PatternLowerScope::RewriteRuleCompile;
    let Some(slots) =
        crate::replace::pattern_slots(g, table, &m.pattern, &mut next_occurrence, scope)
    else {
        return false;
    };
    let (li, ri) = (m.left_switch as usize, m.right_switch as usize);
    if li >= slots.len() || ri >= slots.len() {
        return false;
    }
    !slots.iter().any(|s| {
        matches!(
            s,
            crate::replace::Slot::Alpha { .. } | crate::replace::Slot::Repeat { .. }
        )
    })
}

/// `true` iff `nodes` contains a `PatternNode::Quantifier` at any nesting depth, exhaustively matched.
fn nodes_have_quantifier(nodes: &[pg_grammar::model::PatternNode]) -> bool {
    use pg_grammar::model::PatternNode;
    nodes.iter().any(|n| match n {
        PatternNode::Quantifier { .. } => true,
        PatternNode::Context(_)
        | PatternNode::CharDef(_)
        | PatternNode::Segments { .. }
        | PatternNode::Anchor(_) => false,
    })
}

/// `true` iff `nodes` contains an unbounded `max="-1"` quantifier, recursing into bounded children too since an outer bound alone doesn't prove the whole construct finite.
fn nodes_have_unbounded_quantifier(nodes: &[pg_grammar::model::PatternNode]) -> bool {
    use pg_grammar::model::PatternNode;
    nodes.iter().any(|n| match n {
        PatternNode::Quantifier { max: None, .. } => true,
        PatternNode::Quantifier {
            max: Some(_),
            children,
            ..
        } => nodes_have_unbounded_quantifier(children),
        PatternNode::Context(_)
        | PatternNode::CharDef(_)
        | PatternNode::Segments { .. }
        | PatternNode::Anchor(_) => false,
    })
}

/// `true` iff `r` contains a `Quantifier` anywhere; `characterize`'s trigger for `QuantifierPattern`.
fn rule_has_quantifier(r: &pg_grammar::model::RewriteRuleDef) -> bool {
    if nodes_have_quantifier(&r.lhs.nodes) {
        return true;
    }
    for sr in &r.subrules {
        if nodes_have_quantifier(&sr.rhs.nodes) {
            return true;
        }
        if let Some(p) = &sr.left_env {
            if nodes_have_quantifier(&p.nodes) {
                return true;
            }
        }
        if let Some(p) = &sr.right_env {
            if nodes_have_quantifier(&p.nodes) {
                return true;
            }
        }
    }
    false
}

/// `true` iff `r` contains a genuinely unbounded `Quantifier`; `all_bounded`'s own negation.
fn rule_has_unbounded_quantifier(r: &pg_grammar::model::RewriteRuleDef) -> bool {
    if nodes_have_unbounded_quantifier(&r.lhs.nodes) {
        return true;
    }
    for sr in &r.subrules {
        if nodes_have_unbounded_quantifier(&sr.rhs.nodes) {
            return true;
        }
        if let Some(p) = &sr.left_env {
            if nodes_have_unbounded_quantifier(&p.nodes) {
                return true;
            }
        }
        if let Some(p) = &sr.right_env {
            if nodes_have_unbounded_quantifier(&p.nodes) {
                return true;
            }
        }
    }
    false
}

/// Groups `rhs`'s `Copy(Input(i))`/`Modify(Input(i), _)` occurrences by the input part they
/// reference, mirroring (independently — this crate has no dependency edge onto `pg-rules`'
/// private `morph.rs`, only re-derives the SAME predicate over the SAME frozen `OutputAction`/
/// `PartRef` shapes model.rs already freezes) `pg_rules::morph::redup_part_ref`'s own grouping
/// key. Returns `true` iff some input part is echoed >= 2 times — the actual "true reduplication"
/// trigger (`pg_rules::morph::classify_redup`'s own early-return-empty condition: "if no group has
/// length > 1, nothing here is reduplication at all").
///
/// This is deliberately NOT "does `redup_hint != Implicit`": `ReduplicationHint::Implicit`
/// (model.rs:682) is the DTD's own default value for EVERY non-reduplicating affix subrule
/// (`pg_grammar::load`'s `_ => ReduplicationHint::Implicit` fallback when the `redupMorphType`
/// attribute is simply absent) — treating the hint's mere presence as the trigger would fail-close
/// literally every ordinary affixation grammar ever loaded, which is not what "reduplication"
/// means here and would break the "ordinary grammar characterizes Proven" test.
///
/// # The single authority for "is this reduplication"
/// `pub` because this is now the ONLY definition of the fact in this crate.
/// `crate::backend_registry::Applicability::HasReduplication` and
/// `crate::backend_space::GrammarFacts::reduplicative_allomorphs` each used to carry their own
/// `redup_hint != Implicit || copies > lhs.len()` variant — precisely the hint-keyed trap the
/// paragraph above documents — and so could fire (offering `FAMILY_COPY_BRANCH`, counting
/// reduplicative allomorphs) on grammars where this predicate, `pg_rules::morph::classify_redup`
/// and therefore `crate::peel::ReduplicationPeeler` all agree there is no reduplication at all.
/// Both now consume this function. Note the `pg-grammar` fwdata loader
/// (`pg-grammar/src/compile/affixes.rs`) assigns `ReduplicationHint::Prefix`/`Suffix` from an
/// allomorph's mere MORPH TYPE, so on any fwdata-sourced grammar the old hint-keyed variants fired
/// for every prefix and suffix in the language.
pub fn rhs_has_true_reduplication(rhs: &[OutputAction]) -> bool {
    let mut counts: HashMap<u16, u32> = HashMap::new();
    for action in rhs {
        let part = match action {
            OutputAction::Copy(PartRef::Input(i)) | OutputAction::Modify(PartRef::Input(i), _) => {
                Some(*i)
            }
            _ => None,
        };
        if let Some(i) = part {
            *counts.entry(i).or_insert(0) += 1;
        }
    }
    counts.values().any(|&c| c > 1)
}

/// `true` iff `allo` has a multi-part LHS with a dropped part, not merely any `OutputAction` at all.
fn allomorph_drops_lhs_material(allo: &AffixAllomorphDef) -> bool {
    if allo.lhs.len() <= 1 {
        return false;
    }
    let mut copied: std::collections::HashSet<u16> = std::collections::HashSet::new();
    for action in &allo.rhs {
        if let OutputAction::Copy(PartRef::Input(i)) = action {
            copied.insert(*i);
        }
    }
    (0..allo.lhs.len() as u16).any(|i| !copied.contains(&i))
}

/// Exhaustively matched purely for the no-catch-all discipline; the real trigger is `allomorph_drops_lhs_material`'s structural test, not a per-variant capability difference.
fn output_action_label(action: &OutputAction) -> &'static str {
    match action {
        OutputAction::Copy(_) => "copy",
        OutputAction::InsertSegments { .. } => "insert-segments",
        OutputAction::Modify(_, _) => "modify",
        OutputAction::InsertContext(_) => "insert-context",
    }
}

/// Exhaustively matched purely for the discipline; every variant folds into the same `ConfirmOnly` outcome.
fn co_occurrence_adjacency_label(
    adjacency: pg_grammar::model::CoOccurrenceAdjacency,
) -> &'static str {
    use pg_grammar::model::CoOccurrenceAdjacency as Adj;
    match adjacency {
        Adj::Anywhere => "anywhere",
        Adj::SomewhereToLeft => "somewhere-to-left",
        Adj::SomewhereToRight => "somewhere-to-right",
        Adj::AdjacentToLeft => "adjacent-to-left",
        Adj::AdjacentToRight => "adjacent-to-right",
    }
}

/// Computes `MultiTableDetail`: checks every pair of `char_tables` for a shared NFD representation.
fn multi_table_detail(g: &Grammar) -> MultiTableDetail {
    let table_count = g.char_tables.len();
    let rep_sets: Vec<HashSet<&str>> = g
        .char_tables
        .iter()
        .map(|t| {
            t.iter()
                .flat_map(|(id, _)| t.get(id).representations_nfd().iter().map(String::as_str))
                .collect()
        })
        .collect();

    let mut witness: Option<String> = None;
    'outer: for i in 0..rep_sets.len() {
        for j in (i + 1)..rep_sets.len() {
            if let Some(shared) = rep_sets[i].iter().find(|r| rep_sets[j].contains(*r)) {
                witness = Some(format!(
                    "tables {} and {} both claim representation {shared:?}",
                    g.char_tables[i].xml_id(),
                    g.char_tables[j].xml_id()
                ));
                break 'outer;
            }
        }
    }

    MultiTableDetail {
        table_count,
        representations_pairwise_disjoint: witness.is_none(),
        shared_representation_witness: witness,
    }
}

/// Lowers one `Simultaneous`-mode subrule's span via the rule's own `owning_table`, not `g.char_tables[0]`.
/// See docs/research/pg-foma-capability-design-notes.md.
fn lower_subrule_span(
    g: &Grammar,
    rule: &pg_grammar::model::RewriteRuleDef,
    sr: &pg_grammar::model::RewriteSubruleDef,
) -> Result<(foma::types::Fsm, foma::types::Fsm), String> {
    let table = match crate::replace::owning_table(g, rule) {
        Some(t) => t,
        None => match g.char_tables.len() {
            0 => {
                return Err(
                    "grammar has no CharacterDefinitionTable at all; cannot lower any span"
                        .to_string(),
                );
            }
            1 => &g.char_tables[0],
            n => {
                return Err(format!(
                    "rule {:?} has no owning stratum (owning_table returned None) and the \
                     grammar declares {n} CharacterDefinitionTables -- cannot safely assume \
                     which table's alphabet resolves this rule's natural classes/alpha \
                     variables in a genuinely multi-table grammar; conservatively refusing \
                     rather than guessing table 0 (the fix-multitable-fst-compilation gap this \
                     function used to have)",
                    rule.xml_id
                ));
            }
        },
    };
    let alphabet = crate::replace::SegAlphabet::new(table);
    let opts = foma::options::FomaOptions::default();
    crate::lower::lower_span(
        &opts,
        g,
        &alphabet,
        sr.left_env.as_ref(),
        &rule.lhs,
        sr.right_env.as_ref(),
    )
    .map_err(|reason| reason.to_string())
}

fn characterize_allomorph(
    observations: &mut Vec<CharacteristicObservation>,
    g: &Grammar,
    rule: MRuleId,
    allomorph_index: usize,
    allo: &AffixAllomorphDef,
) {
    // Exhaustive match, unconditional, so a new variant breaks this build regardless of fixtures.
    let _hint_label = match allo.redup_hint {
        ReduplicationHint::Prefix => "prefix",
        ReduplicationHint::Suffix => "suffix",
        ReduplicationHint::Implicit => "implicit",
    };
    if rhs_has_true_reduplication(&allo.rhs) {
        // Re-derives `ReduplicationPeeler::is_reduplication_rule`'s own peel-eligibility test.
        let peel_eligible_rule_kind =
            matches!(g.mrules[rule.0 as usize], MorphRuleDef::AffixProcess(_));
        let peel_attempted = crate::emit::reduplication_rule_is_peelable(g, rule);
        let structural_composite_attempted = crate::emit::is_structural_rule(g, rule);
        observations.push(CharacteristicObservation::new(
            CharacteristicKind::Reduplication,
            ModelLocation::AffixAllomorph {
                rule,
                allomorph_index,
            },
            ObservationDetail::Reduplication(ReduplicationDetail {
                rule,
                allomorph_index,
                peel_eligible_rule_kind,
                peel_attempted,
                structural_composite_attempted,
            }),
        ));
    }

    // Exhaustive match per action, discipline-only; see `output_action_label`'s own doc.
    for action in &allo.rhs {
        let _ = output_action_label(action);
    }
    // `allomorph_drops_lhs_material` cannot see this: it returns early on the single-part ablaut shape.
    if crate::emit::classify_affix(&allo.rhs) == crate::emit::Role::Process {
        observations.push(CharacteristicObservation::new(
            CharacteristicKind::ProcessMorphology,
            ModelLocation::AffixAllomorph {
                rule,
                allomorph_index,
            },
            ObservationDetail::None,
        ));
    }
    if allomorph_drops_lhs_material(allo) {
        observations.push(CharacteristicObservation::new(
            CharacteristicKind::CircumfixOutputAction,
            ModelLocation::AffixAllomorph {
                rule,
                allomorph_index,
            },
            ObservationDetail::CircumfixOutputAction(CircumfixOutputActionDetail {
                rule,
                allomorph_index,
                structural_composite_attempted: crate::emit::is_structural_rule(g, rule),
            }),
        ));
    }

    for co in &allo.co_occurrence {
        let _ = co_occurrence_adjacency_label(co.adjacency);
        observations.push(CharacteristicObservation::new(
            CharacteristicKind::CoOccurrenceConstraint,
            ModelLocation::AllomorphCoOccurrence(allo.id),
            ObservationDetail::None,
        ));
    }
}

/// The stratum index owning `mid`, directly or via a template slot; `None` if not found anywhere.
fn mrule_stratum_rank(g: &Grammar, mid: MRuleId) -> Option<usize> {
    for (si, sd) in g.strata.iter().enumerate() {
        if sd.mrules.contains(&mid) {
            return Some(si);
        }
        for &tid in &sd.templates {
            if g.templates[tid.0 as usize]
                .slots
                .iter()
                .any(|slot| slot.rules.contains(&mid))
            {
                return Some(si);
            }
        }
    }
    None
}

/// The `compounding.non-recursive` vs `compounding.recursive` reachability pass, deliberately coarse.
/// See docs/research/pg-foma-capability-design-notes.md.
fn compounding_recursive(g: &Grammar) -> HashSet<MRuleId> {
    let compounding_rules: Vec<(MRuleId, &pg_grammar::model::CompoundingRuleDef)> = g
        .mrules
        .iter()
        .enumerate()
        .filter_map(|(i, m)| match m {
            MorphRuleDef::Compounding(def) => Some((MRuleId(i as u32), def)),
            _ => None,
        })
        .collect();

    let mut recursive = HashSet::new();
    for &(mid, def) in &compounding_rules {
        if def.max_apps > 1 {
            recursive.insert(mid);
            continue;
        }
        let Some(rank) = mrule_stratum_rank(g, mid) else {
            recursive.insert(mid);
            continue;
        };
        for &(mid2, _) in &compounding_rules {
            if mid2 == mid {
                continue;
            }
            let is_recursive = match mrule_stratum_rank(g, mid2) {
                Some(rank2) => rank2 <= rank,
                None => true,
            };
            if is_recursive {
                recursive.insert(mid);
                break;
            }
        }
    }
    recursive
}

/// Extends `compounding_recursive`'s one-hop boolean into a finite max-depth bound: a rule-count ceiling, not a typological nesting depth (the operative bound is `max_stem_count`, not this).
/// See docs/research/pg-foma-capability-design-notes.md.
fn compounding_max_depth(g: &Grammar) -> HashMap<MRuleId, usize> {
    let compounding_rules: Vec<(MRuleId, &pg_grammar::model::CompoundingRuleDef)> = g
        .mrules
        .iter()
        .enumerate()
        .filter_map(|(i, m)| match m {
            MorphRuleDef::Compounding(def) => Some((MRuleId(i as u32), def)),
            _ => None,
        })
        .collect();

    let max_apps_of = |mid: MRuleId| -> u16 {
        compounding_rules
            .iter()
            .find(|&&(id, _)| id == mid)
            .map(|&(_, def)| def.max_apps)
            .unwrap_or(0)
    };

    // Same one-hop relation `compounding_recursive` tests, factored out to avoid drift between the two.
    let feeds_one_hop = |r2: MRuleId, r: MRuleId| -> bool {
        if r2 == r {
            return max_apps_of(r2) > 1;
        }
        match (mrule_stratum_rank(g, r2), mrule_stratum_rank(g, r)) {
            (Some(rank2), Some(rank)) => rank2 <= rank,
            _ => true,
        }
    };

    let mut result = HashMap::new();
    for &(r, _) in &compounding_rules {
        // Transitive closure BFS over distinct predecessors; terminates on any finite graph, cycles included.
        let mut ancestors: HashSet<MRuleId> = HashSet::new();
        let mut frontier: Vec<MRuleId> = vec![r];
        while let Some(cur) = frontier.pop() {
            for &(r2, _) in &compounding_rules {
                if r2 != cur && feeds_one_hop(r2, cur) && ancestors.insert(r2) {
                    frontier.push(r2);
                }
            }
        }
        // A genuine cycle can rediscover `r` as its own ancestor; remove it to avoid double-counting.
        ancestors.remove(&r);
        let ancestor_sum: usize = ancestors.iter().map(|&r2| max_apps_of(r2) as usize).sum();
        result.insert(r, 1 + max_apps_of(r) as usize + ancestor_sum);
    }
    result
}

thread_local! {
    /// How many times `characterize` has run on this thread, kept thread-local since tests run in parallel.
    /// See docs/research/pg-foma-capability-design-notes.md.
    static CHARACTERIZE_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many times `characterize` has run on the current thread (see `CHARACTERIZE_CALLS`).
/// Test-support only, so a counter no production code needs stays out of the product interface.
#[cfg(feature = "test-support")]
pub fn characterize_call_count() -> u64 {
    CHARACTERIZE_CALLS.with(std::cell::Cell::get)
}

/// Zeroes the current thread's `characterize_call_count`, so a caller can measure one scoped
/// operation rather than a running total. Test-support only, same reasoning as that function.
#[cfg(feature = "test-support")]
pub fn reset_characterize_call_count() {
    CHARACTERIZE_CALLS.with(|c| c.set(0));
}

/// The exhaustive default-deny characterizer: walks `g` and matches EVERY variant of EVERY
/// `model.rs` enum, with no catch-all arm.
///
/// **Not cheap, and not memoized here.** This walk builds real `foma::types::Fsm` networks for
/// every `Simultaneous`-mode subrule (via `lower_subrule_span`). Callers that need the profile
/// more than once -- or need it once per candidate plan -- must go through
/// `crate::grammar_semantics::GrammarSemantics::characteristics`, which computes it exactly once.
/// Every call here is counted in `characterize_call_count`.
pub fn characterize(g: &Grammar) -> CharacteristicsProfile {
    CHARACTERIZE_CALLS.with(|c| c.set(c.get().saturating_add(1)));
    let mut observations = Vec::new();

    // A rule-graph reachability pass, computed once grammar-wide, not a per-rule check.
    let compounding_recursive_set = compounding_recursive(g);
    // The depth-BOUND sibling pass, computed alongside the boolean one above (never replacing it).
    let compounding_max_depth_map = compounding_max_depth(g);

    // --- MorphRuleDef (model.rs:542) --------------------------------------------------------
    for (i, mrule) in g.mrules.iter().enumerate() {
        let id = MRuleId(i as u32);
        match mrule {
            MorphRuleDef::AffixProcess(_) => {
                observations.push(CharacteristicObservation::new(
                    CharacteristicKind::Affixation,
                    ModelLocation::MorphRule(id),
                    ObservationDetail::None,
                ));
            }
            MorphRuleDef::Compounding(_) => {
                // `CompoundingRecursionSafePredicate` reads this detail to decide `ConfirmOnly` vs `Refuse`.
                observations.push(CharacteristicObservation::new(
                    CharacteristicKind::Compounding,
                    ModelLocation::MorphRule(id),
                    ObservationDetail::Compounding(CompoundingDetail {
                        rule: id,
                        recursive: compounding_recursive_set.contains(&id),
                        max_depth: compounding_max_depth_map.get(&id).copied().unwrap_or_else(
                            || panic!("compounding_max_depth must cover every Compounding mrule id, missing {id:?}")
                        ),
                    }),
                ));
            }
            MorphRuleDef::Realizational(_) => {
                observations.push(CharacteristicObservation::new(
                    CharacteristicKind::RealizationalMorphology,
                    ModelLocation::MorphRule(id),
                    ObservationDetail::None,
                ));
            }
        }

        // `AffixProcess`/`Realizational` share one allomorph shape; walk it once via the uniform accessor.
        if let Some(allomorphs) = mrule.affix_allomorphs() {
            for (ai, allo) in allomorphs.iter().enumerate() {
                characterize_allomorph(&mut observations, g, id, ai, allo);
            }
        }

        // Matched exhaustively for discipline; mints no new characteristic (`Compounding` is already captured at the rule level above).
        if let MorphRuleDef::Compounding(def) = mrule {
            for sub in &def.subrules {
                for action in &sub.rhs {
                    let _ = output_action_label(action);
                }
            }
        }
    }

    // --- MorphRuleOrder (model.rs:1057), per stratum ----------------------------------------
    for (i, stratum) in g.strata.iter().enumerate() {
        let id = StratumId(i as u8);
        match stratum.mrule_order {
            MorphRuleOrder::Linear => observations.push(CharacteristicObservation::new(
                CharacteristicKind::OrderedMorphRuleApplication,
                ModelLocation::Stratum(id),
                ObservationDetail::None,
            )),
            MorphRuleOrder::Unordered => observations.push(CharacteristicObservation::new(
                CharacteristicKind::UnorderedMorphRuleApplication,
                ModelLocation::Stratum(id),
                ObservationDetail::UnorderedStratum({
                    let m = crate::unordered::stratum_metrics(id, stratum);
                    UnorderedStratumDetail {
                        stratum: m.stratum,
                        rule_count: m.rule_count,
                    }
                }),
            )),
        }
    }

    // --- MprGroup / MprGroupOutput (model.rs:824-842) ---------------------------------------
    for (i, group) in g.mpr_groups.iter().enumerate() {
        // `MprGroupMatchType` has no disposition of its own; matched anyway so a third variant is forced through this file rather than silently ignored.
        match group.match_type {
            MprGroupMatchType::All | MprGroupMatchType::Any => {}
        }
        match group.output {
            MprGroupOutput::Append => observations.push(CharacteristicObservation::new(
                CharacteristicKind::MprGroupAppend,
                ModelLocation::MprGroup(i),
                ObservationDetail::None,
            )),
            MprGroupOutput::Overwrite => observations.push(CharacteristicObservation::new(
                CharacteristicKind::MprGroupOverwrite,
                ModelLocation::MprGroup(i),
                ObservationDetail::None,
            )),
        }
    }

    // --- PhonRuleDef / RewriteMode / Dir / RewriteSubruleDef (model.rs:402-453) -------------
    for (i, prule) in g.prules.iter().enumerate() {
        let id = PRuleId(i as u32);
        match prule {
            PhonRuleDef::Rewrite(r) => {
                match r.mode {
                    RewriteMode::Iterative => observations.push(CharacteristicObservation::new(
                        CharacteristicKind::IterativeRewrite,
                        ModelLocation::PhonRule(id),
                        ObservationDetail::None,
                    )),
                    RewriteMode::Simultaneous => {
                        let subrules: Vec<SubruleGateInfo> = r
                            .subrules
                            .iter()
                            .enumerate()
                            .map(|(si, sr)| SubruleGateInfo {
                                index: si,
                                required_mpr: sr.required_mpr,
                                excluded_mpr: sr.excluded_mpr,
                                self_opaquing: sr.self_opaquing,
                            })
                            .collect();
                        observations.push(CharacteristicObservation::new(
                            CharacteristicKind::SimultaneousRewrite,
                            ModelLocation::PhonRule(id),
                            ObservationDetail::SimultaneousRewrite(SimultaneousRewriteDetail {
                                rule: id,
                                subrules,
                            }),
                        ));
                    }
                }
                match r.dir {
                    Dir::LeftToRight => observations.push(CharacteristicObservation::new(
                        CharacteristicKind::LeftToRightRewrite,
                        ModelLocation::PhonRule(id),
                        ObservationDetail::None,
                    )),
                    Dir::RightToLeft => {
                        // One call, one shared source of truth: the two fields below can never disagree.
                        let diagnosis = rtl_reversal_diagnosis(g, r);
                        observations.push(CharacteristicObservation::new(
                            CharacteristicKind::RightToLeftRewrite,
                            ModelLocation::PhonRule(id),
                            ObservationDetail::RightToLeftRewrite(RightToLeftRewriteDetail {
                                rule: id,
                                reversal_construction_attempted: diagnosis.is_ok(),
                                unsupported_reason: diagnosis.err().flatten(),
                            }),
                        ));
                    }
                }
                // "Epenthesis" is an empty-`lhs` RULE, on `RewriteRuleDef.lhs`, not a subrule field.
                if r.lhs.nodes.is_empty() {
                    observations.push(CharacteristicObservation::new(
                        CharacteristicKind::Epenthesis,
                        ModelLocation::PhonRule(id),
                        ObservationDetail::None,
                    ));
                }
                for (si, sr) in r.subrules.iter().enumerate() {
                    if sr.required_pos.is_some()
                        || !sr.required_mpr.is_empty()
                        || !sr.excluded_mpr.is_empty()
                    {
                        observations.push(CharacteristicObservation::new(
                            CharacteristicKind::SubruleGating,
                            ModelLocation::RewriteSubrule {
                                rule: id,
                                subrule_index: si,
                            },
                            ObservationDetail::None,
                        ));
                    }
                }
                // A grammar-level structural fact about pattern nodes used, independent of `RewriteMode`/`Dir` (both already characterized above).
                if rule_has_quantifier(r) {
                    observations.push(CharacteristicObservation::new(
                        CharacteristicKind::QuantifierPattern,
                        ModelLocation::PhonRule(id),
                        ObservationDetail::QuantifierPattern(QuantifierPatternDetail {
                            rule: id,
                            all_bounded: !rule_has_unbounded_quantifier(r),
                            compile_attempted: rtl_reversal_construction_attempted(g, r),
                        }),
                    ));
                }
            }
            PhonRuleDef::Metathesis(m) => observations.push(CharacteristicObservation::new(
                CharacteristicKind::Metathesis,
                ModelLocation::PhonRule(id),
                ObservationDetail::Metathesis(MetathesisDetail {
                    rule: id,
                    swap_construction_attempted: metathesis_swap_construction_attempted(g, m),
                }),
            )),
        }
    }

    // --- NaturalClassKind (model.rs:361) ----------------------------------------------------
    for (i, nc) in g.natural_classes.iter().enumerate() {
        let id = NatClassId(i as u32);
        match &nc.kind {
            NaturalClassKind::Feature(_) => observations.push(CharacteristicObservation::new(
                CharacteristicKind::NaturalClassDefinition,
                ModelLocation::NaturalClass(id),
                ObservationDetail::None,
            )),
            NaturalClassKind::Segments(_) => observations.push(CharacteristicObservation::new(
                CharacteristicKind::NaturalClassDefinition,
                ModelLocation::NaturalClass(id),
                ObservationDetail::None,
            )),
        }
    }

    // --- MorphemeCoOccurrenceRuleDef (model.rs:508/521), per morpheme -----------------------
    for (i, morpheme) in g.morphemes.iter().enumerate() {
        for co in &morpheme.co_occurrence {
            let _ = co_occurrence_adjacency_label(co.adjacency);
            observations.push(CharacteristicObservation::new(
                CharacteristicKind::CoOccurrenceConstraint,
                ModelLocation::MorphemeCoOccurrence(i),
                ObservationDetail::None,
            ));
        }
    }

    // Root allomorphs; affix-allomorph co-occurrence is already covered by `characterize_allomorph`.
    for entry in &g.entries {
        for allo in &entry.allomorphs {
            for co in &allo.co_occurrence {
                let _ = co_occurrence_adjacency_label(co.adjacency);
                observations.push(CharacteristicObservation::new(
                    CharacteristicKind::CoOccurrenceConstraint,
                    ModelLocation::AllomorphCoOccurrence(allo.id),
                    ObservationDetail::None,
                ));
            }
            if allo.stem_name.is_some() {
                observations.push(CharacteristicObservation::new(
                    CharacteristicKind::StemName,
                    ModelLocation::RootAllomorph(allo.id),
                    ObservationDetail::None,
                ));
            }
        }
        // Attributed to the first equal pair found (document order), via the real `root_constraints_equal`.
        'pairs: for (i, a) in entry.allomorphs.iter().enumerate() {
            for b in &entry.allomorphs[i + 1..] {
                if pg_rules::validity::root_constraints_equal(a, b) {
                    observations.push(CharacteristicObservation::new(
                        CharacteristicKind::FreeFluctuation,
                        ModelLocation::RootAllomorph(a.id),
                        ObservationDetail::None,
                    ));
                    break 'pairs;
                }
            }
        }
    }

    // Attributed to the first stratum whose table differs from the base, though the detail below is grammar-wide.
    if g.char_tables.len() > 1 {
        let detail = multi_table_detail(g);
        let location = g
            .strata
            .iter()
            .enumerate()
            .find(|(_, s)| s.table != g.strata[0].table)
            .map(|(i, _)| ModelLocation::Stratum(StratumId(i as u8)))
            .unwrap_or(ModelLocation::Stratum(StratumId(0)));
        observations.push(CharacteristicObservation::new(
            CharacteristicKind::MultiTable,
            location,
            ObservationDetail::MultiTable(detail),
        ));
    }

    // One root per observation, through the emitter's own `cross_table_respelling`: the final table's rendering of the root's own bundles, no cascade, so a root both tables spell alike is not flagged however a later rule changes it.
    if g.char_tables.len() > 1 {
        let surface = crate::emit::surface_table(g);
        for sd in &g.strata {
            let table = &g.char_tables[sd.table.0 as usize];
            if std::ptr::eq(table, surface) {
                continue;
            }
            for &entry_id in &sd.entries {
                for allo in &g.entries[entry_id.0 as usize].allomorphs {
                    if allo.is_pattern {
                        continue;
                    }
                    if let Some(surface_spelling) =
                        crate::emit::cross_table_respelling(g, table, &allo.shape.text)
                    {
                        observations.push(CharacteristicObservation::new(
                            CharacteristicKind::CrossTableRespelling,
                            ModelLocation::RootAllomorph(allo.id),
                            ObservationDetail::CrossTableRespelling(CrossTableRespellingDetail {
                                allomorph: allo.id,
                                own_spelling: allo.shape.text.clone(),
                                surface_spelling,
                            }),
                        ));
                    }
                }
            }
        }
    }

    let cardinality = GrammarCardinality {
        entry_count: g.entries.len(),
        morpheme_count: g.morphemes.len(),
        mrule_count: g.mrules.len(),
        prule_count: g.prules.len(),
        stratum_count: g.strata.len(),
        max_derivation_chain_depth: None,
    };

    CharacteristicsProfile {
        observations,
        cardinality,
    }
}

// ---- CapabilityPredicate + PredicateVerdict + EvidenceProvenance + CapabilityDiagnostic ----

/// `PredicateId` and `CapabilityDiagnostic` are plain data the pack format also reads, so they
/// live in `pg-health`; re-exported here at their historical path.
pub use pg_health::capability::{CapabilityDiagnostic, PredicateId};

/// Where a predicate's evidence comes from.
///
/// ADR-0001's "Two-tier, migrating" names two: `structural` for the controllable composition path,
/// and `behavioral` — proven by oracle witnesses — for the black-box foma mainline, so "the two are
/// never conflated". Only `Structural` is representable here, and every predicate and
/// `GrammarWideCheck` in this crate carries it, because each reads model fields or lowered automata
/// directly. The missing tier is a KNOWN GAP, recorded rather than closed: a `Behavioral` variant
/// nothing constructs would be a control that cannot act. It becomes a variant when a predicate
/// actually constrains on an oracle witness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceProvenance {
    /// Evidence comes from directly inspecting compositional structure (lowered automata, or —
    /// as `SimultaneousSubruleOverlapPredicate` does today — directly-readable model fields like
    /// `required_mpr`/`excluded_mpr`/`self_opaquing`).
    Structural,
}

fn kind_wire_name(kind: CharacteristicKind) -> &'static str {
    use CharacteristicKind::*;
    match kind {
        Affixation => "affixation",
        RealizationalMorphology => "realizational_morphology",
        Compounding => "compounding",
        OrderedMorphRuleApplication => "ordered_morph_rule_application",
        UnorderedMorphRuleApplication => "unordered_morph_rule_application",
        MprGroupAppend => "mpr_group_append",
        MprGroupOverwrite => "mpr_group_overwrite",
        IterativeRewrite => "iterative_rewrite",
        SimultaneousRewrite => "simultaneous_rewrite",
        LeftToRightRewrite => "left_to_right_rewrite",
        RightToLeftRewrite => "right_to_left_rewrite",
        Metathesis => "metathesis",
        Epenthesis => "epenthesis",
        SubruleGating => "subrule_gating",
        CircumfixOutputAction => "circumfix_output_action",
        Reduplication => "reduplication",
        CoOccurrenceConstraint => "co_occurrence_constraint",
        NaturalClassDefinition => "natural_class_definition",
        MultiTable => "multi_table",
        CrossTableRespelling => "cross_table_respelling",
        QuantifierPattern => "quantifier_pattern",
        StemName => "stem_name",
        FreeFluctuation => "free_fluctuation",
        ProcessMorphology => "process_morphology",
    }
}

fn kind_from_wire_name(s: &str) -> Option<CharacteristicKind> {
    CharacteristicKind::ALL
        .iter()
        .copied()
        .find(|kind| kind_wire_name(*kind) == s)
}

impl Serialize for CharacteristicKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(kind_wire_name(*self))
    }
}

impl<'de> Deserialize<'de> for CharacteristicKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        kind_from_wire_name(&s)
            .ok_or_else(|| D::Error::custom(format!("unknown CharacteristicKind wire name: {s}")))
    }
}

fn disposition_wire_name(disposition: Disposition) -> &'static str {
    match disposition {
        Disposition::Proven => "proven",
        Disposition::ConfigPredicate => "config_predicate",
        Disposition::ConfirmOnly => "confirm_only",
    }
}

fn disposition_from_wire_name(s: &str) -> Option<Disposition> {
    match s {
        "proven" => Some(Disposition::Proven),
        "config_predicate" => Some(Disposition::ConfigPredicate),
        "confirm_only" => Some(Disposition::ConfirmOnly),
        _ => None,
    }
}

impl Serialize for Disposition {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(disposition_wire_name(*self))
    }
}

impl<'de> Deserialize<'de> for Disposition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        disposition_from_wire_name(&s)
            .ok_or_else(|| D::Error::custom(format!("unknown Disposition wire name: {s}")))
    }
}

fn provenance_wire_name(provenance: EvidenceProvenance) -> &'static str {
    match provenance {
        EvidenceProvenance::Structural => "structural",
    }
}

fn provenance_from_wire_name(s: &str) -> Option<EvidenceProvenance> {
    match s {
        "structural" => Some(EvidenceProvenance::Structural),
        _ => None,
    }
}

impl Serialize for EvidenceProvenance {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(provenance_wire_name(*self))
    }
}

impl<'de> Deserialize<'de> for EvidenceProvenance {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        provenance_from_wire_name(&s)
            .ok_or_else(|| D::Error::custom(format!("unknown EvidenceProvenance wire name: {s}")))
    }
}

/// A capability predicate's verdict for one plan node.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "a capability verdict that is computed and dropped decides nothing"]
pub enum PredicateVerdict {
    /// Proven faithful; admission-filtering allowed.
    Admit,
    /// Propose the superset; no no-false-negative proof. First-class, not a failure.
    ConfirmOnly,
    /// Hard compile-time fail (overridable via the capability override).
    Refuse(CapabilityDiagnostic),
}

/// The compilers that compose `crate::replace`'s rewrite cascade, and so the only ones a
/// `crate::lower::pattern_slots` admission failure can constrain. See docs/research/pg-foma-capability-design-notes.md.
pub const CASCADE_COMPOSING_STRATEGIES: &[EmissionStrategy] = &[
    EmissionStrategy::PlanComposed,
    EmissionStrategy::TemplatedUnderlyingTokens,
];

/// The compilers built on `crate::emit`'s derivation layers. See
/// docs/research/pg-foma-capability-design-notes.md.
pub const DERIVATION_LAYER_STRATEGIES: &[EmissionStrategy] = &[
    EmissionStrategy::TunedSurfaceProbed,
    EmissionStrategy::TemplatedUnderlyingTokens,
];

/// An oracle-verified, conservative proof obligation. Implementors MUST over-refuse rather than
/// under-refuse — the discipline every predicate in this module follows.
pub trait CapabilityPredicate {
    /// e.g. `"simultaneous.subrule-overlap"`.
    fn id(&self) -> PredicateId;
    /// The `crate::advice_catalog` remedy shape this predicate's refusal maps to.
    fn shape_key(&self) -> &'static str;
    /// Which `CharacteristicKind`(s) this predicate claims to discharge.
    fn discharges(&self) -> &[CharacteristicKind];
    /// Which `crate::enumerate::EmissionStrategy`s this predicate's judgement actually constrains —
    /// the compilers whose proposer could exhibit the shape it refuses.
    ///
    /// Defaults to every strategy; narrowing is a behaviour change requiring per-compiler evidence,
    /// and the identity tests move with it. See docs/research/pg-foma-capability-design-notes.md.
    fn constrains_strategies(&self) -> &[EmissionStrategy] {
        ALL_STRATEGIES
    }
    /// This predicate's verdict for `plan_node`, given the grammar-wide `profile` (see this
    /// module's own top-doc for why `plan_node: &PlanNodeKind` rather than the literal
    /// `&PlanNode` — `crate::plan` has no type by that name).
    fn evaluate(
        &self,
        grammar: &Grammar,
        profile: &CharacteristicsProfile,
        plan_node: &PlanNodeKind,
    ) -> PredicateVerdict;
    /// What kind of evidence backs this predicate's verdict.
    fn provenance(&self) -> EvidenceProvenance;
}

// ---- The worked simultaneous.subrule-overlap predicate ----

/// Extracts the `PRuleId` a rewrite-rule leaf is addressed by; any other node shape yields `None`, treated as vacuous `Admit`.
fn rewrite_rule_of(plan_node: &PlanNodeKind) -> Option<PRuleId> {
    match plan_node {
        PlanNodeKind::Leaf {
            fragment: FragmentSpec::RewriteRule { rule },
            ..
        } => Some(*rule),
        _ => None,
    }
}

/// The cheap orthogonality early-out: a sufficient, not necessary, condition for MPR-gate disjointness.
fn mpr_gates_disjoint(a: &SubruleGateInfo, b: &SubruleGateInfo) -> bool {
    a.required_mpr.overlaps(b.excluded_mpr) || b.required_mpr.overlaps(a.excluded_mpr)
}

/// The worked example: a `RewriteRuleDef` with `mode == Simultaneous` is faithfully compilable
/// UNLESS two of its subrules' environments can match at the same input position.
///
/// ```text
/// evaluate(rule):
///   if rule.mode != Simultaneous: return Admit
///   for each unordered pair (s_i, s_j) of rule.subrules:
///       if s_i.self_opaquing || s_j.self_opaquing: return Refuse   # never attempt Admit here
///       if mpr_gates_disjoint(s_i, s_j): continue                  # cheap orthogonality early-out
///       return Refuse                                              # conservative: see below
///   return Admit
/// ```
///
/// # The real automaton intersection
/// The precise test is `intersect(span(s_i), span(s_j))` where `span(s) = left_env · lhs_focus ·
/// right_env`, lowered to an `Fsm` via `crate::lower::lower_span`. Every pair that survives the
/// `self_opaquing`/`mpr_gates_disjoint` early-outs is decided by
/// `crate::lower::spans_overlap` over each subrule's span, lowered here on demand for the pairs
/// that survive the gate early-outs and remembered for the length of one call.
/// `Refuse` only when the intersection is genuinely NON-EMPTY (a real witness
/// overlap), or when a span will not lower at all (a pattern node
/// kind `lower_span` cannot yet represent — any approximation rounds toward `Refuse`, which still
/// applies to THAT residual gap). This `Admit`s strictly more pairs than an unconditional-`Refuse`
/// fallback would (never fewer — over-refusal only ever narrows as proof machinery improves); see
/// this module's test module for a pair that is provably `Admit` under this test.
///
/// # Provenance
/// `EvidenceProvenance::Structural`: `self_opaquing`/`mpr_gates_disjoint` still read directly-
/// inspectable `model.rs` fields for their own early-outs, and the surviving-pair test now
/// genuinely intersects REAL lowered automata (`crate::lower`) — a controllable composition path,
/// not a judgment call: this is not evidence-kind-matches-but-proof-not-yet-built, it IS the
/// controllable-composition proof.
pub struct SimultaneousSubruleOverlapPredicate;

impl CapabilityPredicate for SimultaneousSubruleOverlapPredicate {
    fn id(&self) -> PredicateId {
        "simultaneous.subrule-overlap"
    }

    fn shape_key(&self) -> &'static str {
        "wide-phonology"
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::SimultaneousRewrite]
    }

    // The overlap proof gates `crate::replace`'s sequential compose; the mainline emitter never runs it.
    fn constrains_strategies(&self) -> &[EmissionStrategy] {
        CASCADE_COMPOSING_STRATEGIES
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        grammar: &Grammar,
        profile: &CharacteristicsProfile,
        plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        let Some(rule) = rewrite_rule_of(plan_node) else {
            return PredicateVerdict::Admit;
        };
        let Some(detail) = profile.simultaneous_detail(rule) else {
            // Not observed as Simultaneous at all (e.g. Iterative, which is Proven already).
            return PredicateVerdict::Admit;
        };

        let PhonRuleDef::Rewrite(rule_def) = &grammar.prules[rule.0 as usize] else {
            return PredicateVerdict::Admit;
        };
        match subrules_pairwise_verdict(grammar, rule_def, &detail.subrules) {
            Ok(()) => PredicateVerdict::Admit,
            Err((i, j, witness)) => PredicateVerdict::Refuse(CapabilityDiagnostic {
                predicate: self.id(),
                construct: format!("prule {} subrules {}/{}", rule.0, i, j),
                witness,
            }),
        }
    }
}

/// The per-pair decision, shared by the gate and the compiler so the two cannot disagree.
/// Spans lower on demand, one memo slot each -- docs/research/pg-foma-capability-design-notes.md
fn subrules_pairwise_verdict(
    g: &Grammar,
    rule: &pg_grammar::model::RewriteRuleDef,
    subrules: &[SubruleGateInfo],
) -> Result<(), (usize, usize, String)> {
    let mut lowered: Vec<Option<Result<(foma::types::Fsm, foma::types::Fsm), String>>> =
        (0..subrules.len()).map(|_| None).collect();
    for i in 0..subrules.len() {
        for j in (i + 1)..subrules.len() {
            let a = subrules[i];
            let b = subrules[j];

            // If either subrule is self_opaquing, do not attempt Admit; checked before the mpr-gate early-out.
            if a.self_opaquing || b.self_opaquing {
                return Err((
                    a.index,
                    b.index,
                    format!(
                        "subrule {} and/or {} is self_opaquing (analysis fixpoint reapply); \
                         D3 rounds any self-opaquing pair to Refuse rather than attempt Admit",
                        a.index, b.index
                    ),
                ));
            }

            if mpr_gates_disjoint(&a, &b) {
                continue;
            }

            // This pair survived both early-outs, so it genuinely needs both spans.
            for slot in [i, j] {
                if lowered[slot].is_none() {
                    let sr = &rule.subrules[subrules[slot].index];
                    lowered[slot] = Some(lower_subrule_span(g, rule, sr));
                }
            }
            // A span that will not lower rounds to Refuse, naming the construct, never silently admitting.
            let opts = foma::options::FomaOptions::default();
            let (span_of_a, span_of_b) = (&lowered[i], &lowered[j]);
            match (span_of_a.as_ref(), span_of_b.as_ref()) {
                (Some(Ok(span_a)), Some(Ok(span_b))) => {
                    let overlaps = crate::lower::spans_overlap(&opts, span_a, span_b);
                    if overlaps {
                        return Err((
                            a.index,
                            b.index,
                            format!(
                                "subrules {} and {} are not mpr-gate-disjoint, and their \
                                 lowered left_env/lhs_focus/right_env spans (Stage 1B, \
                                 crate::lower) genuinely intersect at a shared focus \
                                 position -- a real overlap witness, not an unproven \
                                 approximation",
                                a.index, b.index
                            ),
                        ));
                    }
                    // Proven non-overlapping: fall through to the next pair.
                }
                (Some(Err(reason)), _) | (_, Some(Err(reason))) => {
                    return Err((
                        a.index,
                        b.index,
                        format!(
                            "subrules {} and {} are not mpr-gate-disjoint, and at least one \
                             span could not be lowered (Stage 1B, crate::lower): {reason}; \
                             conservatively rounding toward overlap-possible",
                            a.index, b.index
                        ),
                    ));
                }
                // Both slots were just filled, so neither can still be empty.
                (None, _) | (_, None) => unreachable!("both spans are lowered above"),
            }
        }
    }

    Ok(())
}

/// `crate::replace`'s own compile-time consumer of the pairwise-overlap proof: `Ok(())` iff `rule`
/// is either not `Simultaneous` at all (nothing for this check to say — `is_fully_supported_shape`'s
/// caller already treats `Iterative` as unconditionally in-shape) or is `Simultaneous` with
/// subrules proven pairwise non-overlapping, in which case the ADMITTED case's own defining
/// property applies: simultaneous application == sequential application, so `crate::replace`'s
/// existing plain/iterative sequential-compose machinery (fold every subrule's compiled branch via
/// `fsm_compose`, unchanged) is CORRECT for it, not merely reused for convenience. `Err(reason)`
/// otherwise — `crate::replace::compile_rewrite_rule_subset` treats that identically to any other
/// unsupported shape (`None`, honest-unsupported, never a wrong compile).
///
/// Computed FRESH against `g`/`rule` directly (no pre-built `CharacteristicsProfile` needed) --
/// this runs at actual COMPILE time (once per rule), not characterization time (once per plan
/// node the walk visits, `node_decision`'s own doc), so re-deriving `SubruleGateInfo` here
/// (rather than requiring a caller to have already run `characterize`) is the right cost
/// tradeoff, and lets a caller ask this question for one rule without characterizing the whole
/// grammar. Reuses `lower_subrule_span` (see that function's own `owning_table` doc) and
/// `subrules_pairwise_verdict` (the SAME overlap algorithm the capability GATE's own
/// `SimultaneousSubruleOverlapPredicate` uses) — one shared proof, two call sites, so the gate
/// and the compiler can never disagree about which configurations are faithful.
///
/// # Stricter than the registered predicate's own pairwise algorithm, by one case
/// The pairwise loop has no PAIR to examine when `rule.subrules.len() < 2`, so
/// `SimultaneousSubruleOverlapPredicate` itself vacuously `Admit`s a *lone* self-opaquing
/// subrule — correct for that predicate's own proof obligation (subrule-vs-subrule overlap only),
/// but not sufficient for this function's SEPARATE obligation (never compile a configuration whose
/// faithfulness against the actual confirm engine cannot be established): a self-opaquing subrule
/// needs `pg_rules::rewrite`'s analysis-side repeat-until-fixpoint wrapper to be faithfully
/// ANALYZED, which the plain/iterative sequential-compose path this function admits into does not
/// reproduce (one pass, never a fixpoint loop) — so this function refuses ANY self-opaquing
/// subrule unconditionally, even one with no peer to overlap with. Strictly MORE conservative than
/// the registered predicate's own algorithm (over-refuses further, never under-refuses) — the same
/// discipline every predicate in this module already holds itself to; that predicate is
/// intentionally left unchanged by this addition, since touching its published algorithm/tests for
/// a case no existing fixture exercises was judged unnecessary risk.
pub(crate) fn simultaneous_rule_admitted_for_compile(
    g: &Grammar,
    rule: &pg_grammar::model::RewriteRuleDef,
) -> Result<(), String> {
    if rule.mode != RewriteMode::Simultaneous {
        return Ok(());
    }

    for (si, sr) in rule.subrules.iter().enumerate() {
        if sr.self_opaquing {
            return Err(format!(
                "subrule {si} is self_opaquing with no peer subrule for D3's own pairwise \
                 overlap check to ever examine (rule has {} subrule(s)) -- the plain/iterative \
                 sequential-compose path this function admits into never reapplies to a \
                 fixpoint (rust/docs/p13-simultaneous-design.md §4.3/§4.4), so a self-opaquing \
                 subrule is refused here even though D3's own pairwise predicate has nothing to \
                 say about a subrule with no peer",
                rule.subrules.len()
            ));
        }
    }

    let subrules: Vec<SubruleGateInfo> = rule
        .subrules
        .iter()
        .enumerate()
        .map(|(si, sr)| SubruleGateInfo {
            index: si,
            required_mpr: sr.required_mpr,
            excluded_mpr: sr.excluded_mpr,
            self_opaquing: sr.self_opaquing,
        })
        .collect();

    subrules_pairwise_verdict(g, rule, &subrules)
        .map_err(|(i, j, witness)| format!("subrules {i} and {j}: {witness}"))
}

// ---- MultiTable: the config-predicate `fix-multitable-fst-compilation` registers ----

/// The capability predicate for `MultiTable`: a `Grammar` with more than one
/// `CharacterDefinitionTable` is faithfully compilable by `pg_foma::replace` now that every
/// rewrite rule resolves its own natural classes/alpha variables against ITS OWNING stratum's
/// table (`owning_table`, never an implicit `char_tables[0]` default), and a SHARED representation
/// across two tables is now ALSO faithful, via render-time cross-table representation aliasing
/// (`crate::replace::RepresentationAliasMap`, `crate::replace::SegAlphabet::render_tokens`,
/// consumed by `crate::lower::render_slots`'s `Slot::Fixed`/`Slot::Union` arms), never `Refuse`.
///
/// # Why representation-disjointness is the proof obligation — the FALSE-NEGATIVE direction, not
/// the false-positive one
/// `pg_foma::replace::SegAlphabet::token` is a PURE function of a `CharDefId`'s raw per-table
/// index (`PUA_BASE + cd.0`), not of which table that index came from. Two tables sharing a
/// representation `s` at DIFFERENT raw indices produce DIFFERENT tokens for the SAME spelling, so
/// table B's rule — rendered using only table B's own token for `s` — simply produces no match on
/// table-A-originated material spelled `s`. That is a FALSE NEGATIVE: under the
/// propose-and-confirm invariant, the one error class that can never be recovered downstream (a
/// proposer may over-approximate freely; an omission is final). A coincidental raw-index
/// COLLISION (two DIFFERENT spellings landing on the same raw index across tables) is the SAFE
/// direction instead — `pg_rules::rewrite` (the oracle, resolving every rule via an explicit
/// `TableId` with no PUA collapsing at all) already prunes it, which is precisely why this
/// predicate lands at `ConfirmOnly` rather than `Admit` in every case, not only the
/// shared-representation one.
///
/// The fix (`RepresentationAliasMap`) keeps tokens keyed by `(table, char-def)` exactly as before,
/// and only ADDS alternatives at render time: when a normalized representation appears in more than
/// one table, every atom for it renders as a union over every table's own token for that same
/// spelling (`SegAlphabet::render_tokens`). Table B's rule now renders `[τ_B(s) | τ_A(s)]`, fires on
/// A-originated material too, over-approximates, and confirm prunes the extra firings exactly like
/// it already prunes the coincidental-collision case — recall-safe by construction, since aliasing
/// only ever adds candidate tokens to an atom, never removes the atom's own.
///
/// **Metathesis needs a different mechanism than rewrite rules.** `crate::replace::
/// compile_metathesis_swap_net` renders tokens via a direct `alphabet.token(cd)` call, not
/// through `crate::lower::render_slots`, so text-level render-time unioning (the rewrite-rule fix
/// above) is UNSAFE for a `PhonRuleDef::Metathesis` rule sharing a representation across tables
/// (see `crate::replace`'s own module doc, "Cross-table representation aliasing" section under
/// "Metathesis", for the full argument: independently unioning a matched LHS position and its
/// swapped RHS position would let the compiled transducer pair a matched alias with a DIFFERENT
/// alias's token — a new correctness bug, not merely a missed optimization). Instead,
/// `crate::replace::slot_candidates` alias-expands each switch position's own candidate
/// `CharDefId` SET (every `(table, cd)` pair sharing a member's normalized representation, via the
/// SAME `RepresentationAliasMap`), and the pre-existing per-branch cross-product construction (built
/// for exactly this "reproduce the SAME matched value at its swapped position" reason,
/// `resolve_alpha_tuples`'s own identity-preservation precedent) does the rest unmodified: each
/// branch fixes ONE concrete candidate per position and the swap only permutes that literal
/// assignment, so switch-position identity holds by the SAME argument that already covers ordinary
/// (non-aliased) multi-member natural classes — extended one level, not weakened.
/// `MultiTable`'s own `ModelLocation`/`multi_table_detail` are grammar-wide, not rule-kind-specific
/// (this predicate's own "Node applicability" section below), so this predicate could never
/// distinguish "the risky rule is a Rewrite" from "the risky rule is a Metathesis" anyway — moot now
/// that both kinds are covered by the same recall argument. In practice this was advisory-only
/// exposure even before the fix, never a live compile-blocking gap: `CompileDecision` is check-only
/// (`crate::backend_selection::best_case_across_backends`'s own doc — nothing here alters what
/// `emit.rs`/`gate.rs`/`replace.rs`/`preexpand.rs` actually compile), so
/// `crate::replace::compile_metathesis_rule` already compiles whatever it can either way; only the
/// ADVISORY verdict this predicate reports is what's affected.
///
/// # Disposition
/// - **Zero or one table observed at all:** vacuously `Admit` (this predicate has nothing to say —
///   `Disposition::Proven` already covers the ordinary single-table case, the resting
///   disposition for every characteristic the grammar never exercises).
/// - **Two or more tables, ANY relationship between their representations (disjoint OR shared):**
///   `PredicateVerdict::ConfirmOnly` — per-rule table-correct resolution (`owning_table`) plus,
///   for a shared representation, render-time aliasing (`RepresentationAliasMap`) together rule out
///   the false-negative risk for rewrite rules; the residual false-positive risk (raw-index
///   collision, disjoint OR shared) is exactly what the oracle (`pg_rules::rewrite`) already prunes
///   downstream. No PROVEN no-false-positive admission-filter argument exists, so this stays
///   confirm-only-by-default in every case — never `Refuse` for a shared representation.
///
/// # Provenance
/// `EvidenceProvenance::Structural`: `multi_table_detail`'s pairwise-representation check reads
/// directly-inspectable `CharDefTable`/`CharDef` data, no oracle witnesses needed to derive it (the
/// oracle IS still what discharges the `ConfirmOnly` verdict's own recall obligation, per the
/// module doc above, but the PREDICATE's own verdict is a structural fact about the tables
/// themselves).
///
/// # Node applicability
/// Grammar-wide, not node-specific (the same "no corresponding `PlanNodeKind`" shape this module's
/// own `compose_envelope` doc names — `MultiTable`'s own
/// `ModelLocation` is attributed to a representative stratum, but the DETAIL this predicate reads
/// is grammar-wide): `evaluate` ignores `plan_node` entirely and returns the SAME verdict at every
/// node the walk visits, which is safe (`node_decision`'s own doc: a predicate whose construct is
/// absent is a no-op everywhere; here the construct, when present, gates the WHOLE grammar
/// identically at every node, so calling it repeatedly is idempotent under `meet`).
pub struct MultiTableFaithfulThreadingPredicate;

impl CapabilityPredicate for MultiTableFaithfulThreadingPredicate {
    fn id(&self) -> PredicateId {
        "multi-table.faithful-table-threading"
    }

    fn shape_key(&self) -> &'static str {
        "wide-phonology"
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::MultiTable]
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        _grammar: &Grammar,
        profile: &CharacteristicsProfile,
        _plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        let Some(_detail) = profile.multi_table_detail() else {
            // Not observed at all (<= 1 table) -- nothing for this predicate to say (module doc).
            return PredicateVerdict::Admit;
        };
        // A shared representation's false-negative risk is closed at render time; metathesis is the one still-open case, so `ConfirmOnly` covers every multi-table grammar either way.
        PredicateVerdict::ConfirmOnly
    }
}

// ---- RightToLeftRewrite: the config-predicate `compile-right-to-left-rewrites` registers ----

/// The capability predicate for `RightToLeftRewrite`: a `Dir::
/// RightToLeft` rewrite rule is now faithfully COMPILABLE (never a silent LTR mis-compile) by
/// `crate::replace::compile_rtl_branch_net`'s reversal-plus-safety-net-union construction
/// (that function's own doc), PROVIDED the rule's own LHS/RHS/environment patterns are within the
/// shape this crate's compiler already requires for ANY rewrite rule ([`RightToLeftRewriteDetail::
/// reversal_construction_attempted`], computed once by `rtl_reversal_construction_attempted`).
///
/// # Disposition
/// - **Not observed as `Dir::RightToLeft` at all** (e.g. `LeftToRight`, or this predicate asked
///   about a non-rewrite-rule node): vacuously `Admit` — nothing for this predicate to say
///   (mirrors `SimultaneousSubruleOverlapPredicate`'s own "not applicable here" convention).
/// - **Pattern shape within scope** (`reversal_construction_attempted == true`):
///   `PredicateVerdict::ConfirmOnly` — the reversal-plus-union construction is a proven SAFE
///   OVER-APPROXIMATION relative to today's confirm engine (module doc on `compile_rtl_branch_net`:
///   the safety-net `LeftToRight`-style branch alone is already recall-complete against
///   `pg_rules::rewrite`'s own, empirically-verified direction-blind pick-order; the genuinely-
///   reversed branch only ever ADDS candidates, never drops one), but no PROVEN no-false-positive
///   admission-filter argument exists — so this is confirm-only-by-default, never `Admit`.
/// - **Pattern shape outside scope** (`reversal_construction_attempted == false` — the REMAINING
///   reasons are: the rule's own LHS/RHS/environment carries an ambiguous disagree-polarity alpha
///   var (`crate::lower::UnsupportedPatternNode::AlphaAmbiguousDisagree`'s own doc), contains
///   a malformed `Quantifier` (inverted, alpha-nested, or empty-children -- a genuinely UNBOUNDED
///   quantifier is no longer out of scope), or has no resolvable owning table. Same-table or
///   table-qualified cross-table `Segments` and any `Anchor` no longer trigger `Refuse` at all
///   (`crate::lower::PatternLowerScope::RewriteRuleCompile`)):
///   `PredicateVerdict::Refuse`, NAMING the exact failing shape via
///   `RightToLeftRewriteDetail::unsupported_reason` — the real compiler already honestly skips
///   (`None`) exactly this rule (never a silent LTR fallback), so a grammar depending on it
///   must be refused rather than silently missing recall; overridable via the capability override.
///
/// # Provenance
/// `EvidenceProvenance::Structural`: `rtl_reversal_construction_attempted` reads directly-
/// inspectable `model.rs`/`CharDefTable` data (the same structural facts [`crate::replace::
/// pattern_slots`]/`crate::replace::owning_table` already compute for the real compile), no
/// oracle witnesses needed to derive the VERDICT itself — the safe-superset recall ARGUMENT this
/// predicate's own `ConfirmOnly` verdict rests on was separately, empirically verified (this
/// crate's `tests/phase_c_right_to_left.rs`), the same "oracle verified the construction, the
/// predicate reads structure" split `MultiTableFaithfulThreadingPredicate`'s own doc draws.
///
/// # Node applicability
/// Like `SimultaneousSubruleOverlapPredicate`, addressed via `rewrite_rule_of` at a rewrite-
/// rule leaf node — the SAME plan-node-extraction helper, reused rather than re-derived.
pub struct RightToLeftRewriteFaithfulReversalPredicate;

impl CapabilityPredicate for RightToLeftRewriteFaithfulReversalPredicate {
    fn id(&self) -> PredicateId {
        "right-to-left-rewrite.faithful-reversal-construction"
    }

    fn shape_key(&self) -> &'static str {
        "wide-phonology"
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::RightToLeftRewrite]
    }

    // `compile_rtl_branch_net` is a cascade construction; the mainline emitter composes no cascade.
    fn constrains_strategies(&self) -> &[EmissionStrategy] {
        CASCADE_COMPOSING_STRATEGIES
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        _grammar: &Grammar,
        profile: &CharacteristicsProfile,
        plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        let Some(rule) = rewrite_rule_of(plan_node) else {
            return PredicateVerdict::Admit;
        };
        let Some(detail) = profile.right_to_left_detail(rule) else {
            // Not observed as `Dir::RightToLeft` at all; nothing for this predicate to say.
            return PredicateVerdict::Admit;
        };
        if !detail.reversal_construction_attempted {
            // Names the specific failing shape rather than a laundry-list "could be any of these" message.
            let witness = match detail.unsupported_reason {
                Some(reason) => format!(
                    "this rule's own LHS/RHS/environment pattern needs a construct \
                     `crate::replace::pattern_slots` does not support (under \
                     `crate::lower::PatternLowerScope::RewriteRuleCompile`): {reason} -- the real \
                     compiler already honestly skips (None) this exact rule rather than \
                     silently mis-compiling it"
                ),
                None => "this rule has no resolvable owning character-definition table -- the \
                          real compiler already honestly skips (None) this exact rule rather \
                          than silently mis-compiling it"
                    .to_string(),
            };
            return PredicateVerdict::Refuse(CapabilityDiagnostic {
                predicate: self.id(),
                construct: format!("prule {} (Dir::RightToLeft)", rule.0),
                witness,
            });
        }
        PredicateVerdict::ConfirmOnly
    }
}

// ---- Metathesis: the config-predicate `compile-fst-metathesis` registers ----

/// The capability predicate for `Metathesis`: a `PhonRuleDef::Metathesis`
/// rule is faithfully COMPILABLE via `crate::replace::compile_metathesis_rule`'s dedicated swap
/// relation (that function's own module doc: a per-branch literal cross-product union, mirroring
/// `resolve_alpha_tuples`'s own identity-preservation fix) for a `pattern_slots`-acceptable shape,
/// EITHER `Dir`: `Dir::RightToLeft` mirrors the pattern, remaps the two switch indices, reverses,
/// and unions with the plain net -- the SAME construction `compile_rtl_branch_net` uses for RTL
/// rewrite rules (that function's own module doc, "`Dir::RightToLeft`" section, has the full
/// derivation this predicate's disposition below relies on). Any pattern needing
/// `Quantifier`/`Segments`/`Anchor`/`Slot::Alpha`/`Slot::Repeat`
/// anywhere, or with no resolvable owning table, stays unsupported
/// (`crate::replace::compile_metathesis_rule` itself returns `None`, honestly skipped) --
/// direction was never what made those shapes unsupported.
///
/// **Cross-table shared-representation recall.** `crate::replace::slot_candidates` alias-expands
/// every switch position's own candidate set via the SAME `RepresentationAliasMap`
/// `MultiTableFaithfulThreadingPredicate`'s own rewrite-rule fix uses, so a `MetathesisRule` in a
/// grammar whose tables share a normalized representation is not exposed to the false negative
/// that would otherwise result. This predicate's own disposition is unaffected either way
/// (`ConfirmOnly` is already the ceiling for `swap_construction_attempted == true`, cross-table or
/// not) -- alias-expansion only ever REMOVES a recall gap this predicate's `ConfirmOnly` verdict
/// already had to tolerate, never changes which shapes this predicate itself admits or refuses.
///
/// # Disposition
/// - **Not observed as `PhonRuleDef::Metathesis` at all**: vacuously `Admit` — nothing for this
///   predicate to say (mirrors `RightToLeftRewriteFaithfulReversalPredicate`'s own "not
///   applicable here" convention).
/// - **Pattern shape within scope** (`swap_construction_attempted == true`, EITHER `Dir`):
///   `PredicateVerdict::ConfirmOnly` — never `Admit`, for two independent reasons layered on top
///   of each other. `Dir::LeftToRight`'s own cross-product swap-relation construction is a proven
///   SAFE, FAITHFUL FST compile for the SUPPORTED case (`tests/phase_c_metathesis.rs`'s
///   `metathesis_adjacent_singleton_swap_matches_oracle_exactly` proves oracle-EXACT equality
///   against `pg_rules::metathesis`, not merely a safe superset) but still has no PROVEN
///   no-false-negative admission-filter argument. `Dir::
///   RightToLeft` additionally unions in the reversed-mirror branch (module doc above) — a proven
///   SUPERSET of the true RTL relation, sound under propose-and-confirm (the proposer may
///   over-approximate; it must never omit — `tests/phase_c_metathesis.rs`'s own `Dir::RightToLeft`
///   containment witness checks exactly this), but NOT proven exact — the SAME reason RTL rewrite
///   is `ConfirmOnly` rather than `Admit`. Either way, confirm-only-by-default, the same landing
///   spot every other `ConfigPredicate` characteristic in this registry already uses.
/// - **Pattern shape outside scope** (`swap_construction_attempted == false` — an unresolvable
///   owning table, `left_switch == right_switch` or out of bounds, or a pattern shape
///   `crate::replace::pattern_slots` does not accept (a `Slot::Alpha`/`Slot::Repeat`
///   occurrence) — `crate::replace::compile_metathesis_rule`'s own module doc, "Scope" section, has
///   the full, evidence-based account of which of these is genuinely reachable): [`PredicateVerdict
///   ::Refuse`] — the real compiler already honestly skips (`None`) exactly this rule, never a
///   silent wrong compile; overridable via the capability override.
///
/// # Provenance
/// `EvidenceProvenance::Structural`: `swap_construction_attempted` reads directly-inspectable
/// `model.rs`/`CharDefTable` data (the same structural facts `crate::replace::
/// compile_metathesis_rule` itself checks before ever rendering an xre regex), no oracle witnesses
/// needed to derive the VERDICT itself — the safe-recall argument for the SUPPORTED case (oracle-
/// exact for `Dir::LeftToRight`; a proven safe superset, not proven exact, for `Dir::RightToLeft`)
/// was separately, empirically verified against `pg_rules::metathesis` (this crate's own
/// containment fixtures for both directions), the same "oracle verified the construction, the
/// predicate reads structure" split `RightToLeftRewriteFaithfulReversalPredicate`/
/// `MultiTableFaithfulThreadingPredicate`'s own docs draw.
///
/// # Node applicability
/// Like `RightToLeftRewriteFaithfulReversalPredicate`/`QuantifierBoundedExpansionPredicate`,
/// addressed via `rewrite_rule_of` at a rewrite-rule leaf node — the SAME plan-node-extraction
/// helper reused rather than re-derived: `FragmentSpec::RewriteRule { rule: PRuleId }` is generic
/// across BOTH `PhonRuleDef` variants (`crate::plan`'s own doc: "a single rewrite rule's
/// transducer, addressed by its `PRuleId`" — no variant-specific fragment kind exists), so a
/// `PhonRuleDef::Metathesis` leaf uses the identical node shape a `PhonRuleDef::Rewrite` leaf does.
pub struct MetathesisFaithfulSwapPredicate;

impl CapabilityPredicate for MetathesisFaithfulSwapPredicate {
    fn id(&self) -> PredicateId {
        "metathesis.faithful-swap-construction"
    }

    fn shape_key(&self) -> &'static str {
        "wide-phonology"
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::Metathesis]
    }

    // `compile_metathesis_rule` is a cascade construction; the mainline emitter composes no cascade.
    fn constrains_strategies(&self) -> &[EmissionStrategy] {
        CASCADE_COMPOSING_STRATEGIES
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        _grammar: &Grammar,
        profile: &CharacteristicsProfile,
        plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        let Some(rule) = rewrite_rule_of(plan_node) else {
            return PredicateVerdict::Admit;
        };
        let Some(detail) = profile.metathesis_detail(rule) else {
            // Not observed as `PhonRuleDef::Metathesis` at all; nothing for this predicate to say.
            return PredicateVerdict::Admit;
        };
        if !detail.swap_construction_attempted {
            return PredicateVerdict::Refuse(CapabilityDiagnostic {
                predicate: self.id(),
                construct: format!("prule {} (MetathesisRule)", rule.0),
                witness: "this rule's own pattern needs a construct crate::replace::pattern_slots \
                          does not support (Quantifier/Segments/Anchor), carries a Slot::Repeat \
                          occurrence (DTD-legal inside a \
                          MetathesisRule's own PhoneticSequence, though never attested in any \
                          fixture this crate has authored -- Slot::Alpha is additionally \
                          structurally IMPOSSIBLE here, not merely unsupported: \
                          pg_grammar::load::load_metathesis_rule resolves every node against an \
                          EMPTY VarTable, so any AlphaVariable inside a MetathesisRule errors the \
                          whole grammar load before reaching this predicate at all), or has no \
                          resolvable owning character-definition table -- NOT a direction, since \
                          Dir::RightToLeft compiles via the same mirror-and-reverse construction \
                          compile_rtl_branch_net uses for RTL rewrite rules -- the real compiler \
                          already honestly skips (None) this exact rule rather than silently \
                          mis-compiling it"
                    .to_string(),
            });
        }
        PredicateVerdict::ConfirmOnly
    }
}

// ---- CircumfixOutputAction: the config-predicate `cover-circumfix-null-output-actions` registers ----

/// The capability predicate for `CircumfixOutputAction`: an
/// `AffixAllomorphDef` whose RHS drops real LHS material — a circumfix wrapping the stem, a
/// null-role subtractive input (an LHS part matched for context but never copied to the output),
/// or an ordered multi-`InsertSegments` output-action sequence built on top of either shape — is
/// now faithfully COMPILABLE whenever its owning rule reaches [`crate::emit::
/// build_structural_composites`] (`crate::emit::is_structural_rule`'s own admission test): that
/// mechanism resynthesizes every candidate surface via the REAL morphological engine
/// (`pg_rules::morph::synthesize`) rather than splicing literal text, so it is faithful for
/// whatever concrete `OutputAction` sequence a covered rule's allomorphs actually carry — including
/// an ordered-multi-insert fix (`crate::emit::insert_action_texts`, "never silently reduced to the
/// first inserted segment") for the allomorphs that stay on the ordinary (non-structural) emission
/// path.
///
/// A rule stays OUTSIDE `is_structural_rule`'s admitted set only when NONE of its allomorphs
/// classifies `Role::CircumfixPrefix`, NONE drops LHS material while ITSELF classifying
/// `Role::None`/`Prefix`/`Suffix`/`Infix` (checked per allomorph, never gated by what some OTHER
/// allomorph of the same rule classifies as — census C5, below), and the rule carries no
/// `OutputAction::Modify`/`InsertContext` action anywhere — e.g. a rule whose RHS uses
/// `OutputAction::Modify`/`InsertContext` (ablaut/simulfix-style "process morphs", never compilable
/// as literal strings) is never routed there, and the ordinary emission path already honestly
/// reports it `uncovered` (`crate::emit::emit_rule_allomorphs`'s `has_unemittable_action` check)
/// rather than silently mis-compiling it. `is_structural_rule` scans every allomorph for
/// `CircumfixPrefix` and for a dropping `None`/`Prefix`/`Suffix`/`Infix` shape independently (a
/// rule whose allomorph 0 is some OTHER role but a later allomorph is circumfix-shaped, or drops
/// LHS material under one of those four roles, is still admitted), so the exclusion above is
/// exhaustive.
///
/// An RHS that is simultaneously circumfixing (insert before the first `Copy`, insert after the
/// last) AND infixing (a non-`Copy` action strictly between two `Copy`s) classifies
/// `CircumfixPrefix` rather than `Infix`, so it is admitted here instead of being routed to
/// `crate::preexpand`.
///
/// This is NOT primarily a raw-recall fix — checked empirically, not merely reasoned about:
/// `crate::preexpand::extend` (its own module doc) ALSO calls `pg_rules::morph::synthesize_cached`,
/// the SAME real engine `build_structural_composites` uses, so an `Infix`-misclassified rule with
/// this exact shape is ALREADY correctly resynthesized by `crate::preexpand` regardless (confirmed
/// by temporarily reverting `classify_affix`'s reordering and re-running
/// `rust/crates/pg-foma/tests/circumfix_candidate_selection.rs`'s
/// `circumfix_infix_interior_action_recall_parity` — it passes either way). What actually changes
/// is OWNERSHIP, not recall (the same test file's `circumfix_infix_ownership_handoff_is_clean` DOES
/// fail without the reordering): `crate::preexpand` relinquishes the rule and
/// `build_structural_composites` claims it instead. That matters here specifically because THIS
/// predicate reads `is_structural_rule` as its own ground truth for
/// `structural_composite_attempted`: a rule misclassified `Infix` here would make this predicate
/// `Refuse` a grammar `crate::preexpand` was already covering correctly — an over-refusal (never a
/// silent overclaim), consistent with every one of these gaps failing in the honest, fail-closed
/// direction. `build_structural_composites` remains the architecturally correct home regardless:
/// its `CircumfixPrefix` admission is unconditional (`is_structural_rule`'s own comment), where
/// `crate::preexpand`'s Infix coverage of this shape is real but incidental to a module whose own
/// doc scopes it to interdigitation/boundary-fusion, never to circumfix.
///
/// A genuinely `Infix`-classified allomorph — never circumfixing on any allomorph of its own rule —
/// that drops LHS material is a 4th case, closing the census
/// (`docs/research/circumfix-composite-precedence-census.md`, C4): `is_structural_rule` admits it
/// directly via its own drop-aware match arm, never via a `classify_affix` reclassification (unlike
/// C3 above, `rule_role` genuinely stays `Role::Infix`). This one has real recall consequences, not
/// merely a relabeling of an outcome `crate::preexpand` already reached: that module's own
/// resynthesis of this shape is real but incidental (bounded by its own enumeration budget/pruning,
/// not a proven exact-containment argument for every grammar), so `preexpand.rs`'s own
/// `candidate_rules` now excludes any `Infix` rule `is_structural_rule` claims, making
/// `build_structural_composites`'s oracle-backed construction — not `crate::preexpand`'s probe —
/// the mechanism `structural_composite_attempted` reports here, pinned by
/// `tests/phase_c_circumfix.rs::infix_with_drop_structural_recall_parity`'s own
/// candidate-set-membership assertion.
///
/// A 5th case, closing the census further (`docs/research/circumfix-composite-precedence-census.md`,
/// C5): a rule whose FIRST allomorph classifies `Role::Reduplication` (so `crate::emit::rule_role`,
/// which reads only that first allomorph, calls the whole rule `Reduplication`) but which carries a
/// LATER allomorph that drops LHS material while ITSELF classifying `None`/`Prefix`/`Suffix`/
/// `Infix` used to be categorically excluded, exactly as the C1 non-first-allomorph bug excluded a
/// circumfix declared second — the real-world grammar that surfaced this had such an allomorph
/// sitting in its own uncovered list ("mrule 189 allomorph #4"). `is_structural_rule` now checks
/// every allomorph's OWN classification against the drop test, never gated by any other allomorph's
/// role, so this later allomorph is admitted regardless of what allomorph 0 classifies as.
///
/// An RHS that is simultaneously circumfixing AND reduplicating (some `Copy`d part echoed >= 2
/// times) ALSO classifies `CircumfixPrefix` rather than `Reduplication`, so
/// `structural_composite_attempted` is `true` for it too. Unlike the infixing case above, this one
/// is NOT merely an ownership relabeling of an already-correct outcome:
/// `crate::peel::ReduplicationPeeler`'s four scan kinds (that module's own doc) are each a
/// one-sided surface-string match and have no shape that recalls a genuine
/// wrap-both-sides-plus-reduplication surface, so such a rule (when `AffixProcess`-kind, i.e.
/// peel-eligible per `ReduplicationPeelSupportedPredicate`) would otherwise risk a REAL recall
/// gap dressed up as a `ConfirmOnly` verdict — the peel would claim it but could not actually
/// recall it. `build_structural_composites` resynthesizes it correctly instead (same
/// shape-agnostic replay of `pg_rules::morph::synthesize` this predicate's other paragraphs already
/// make), proven non-vacuous by `tests/circumfix_candidate_selection.rs`'s dedicated section (full
/// proposer-to-confirm containment against `pg_parse::Morpher` for a real
/// circumfix-plus-reduplication surface, plus a check that
/// `crate::peel::ReduplicationPeeler::new(&g).has_redup_rules()` is `false` for that same grammar —
/// the peel relinquishes the rule cleanly). See `ReduplicationPeelSupportedPredicate`'s own doc
/// for why its `peel_eligible_rule_kind` field can still read `true` for an `AffixProcess` rule
/// with this exact combined shape without that being a stale or false claim.
///
/// # Disposition
/// - **Not observed at all** (no allomorph drops LHS material anywhere in the grammar): vacuously
///   `Admit` — nothing for this predicate to say (mirrors `RightToLeftRewriteFaithfulReversalPredicate`'s
///   own "not applicable here" convention).
/// - **Every observed occurrence reaches `build_structural_composites`**
///   (`structural_composite_attempted == true` for every `CircumfixOutputActionDetail`):
///   `PredicateVerdict::ConfirmOnly` — the structural-composite construction is a proven faithful,
///   oracle-backed compile for the SUPPORTED case (this change's own containment fixture proves
///   oracle-exact equality against `pg_parse::Morpher` for a covered circumfix/null-role rule,
///   mirroring `MetathesisFaithfulSwapPredicate`'s own "exact containment, not merely a safe
///   superset" precedent), but no PROVEN no-false-negative admission-filter argument exists —
///   confirm-only-by-default, the same landing spot every other `ConfigPredicate` characteristic in
///   this registry already uses.
/// - **At least one observed occurrence does NOT reach `build_structural_composites`**
///   (`structural_composite_attempted == false`): `PredicateVerdict::Refuse` — the real compiler
///   already honestly skips this exact allomorph everywhere (module doc above), never a silent wrong
///   compile, but a grammar depending on it must be refused rather than silently missing recall;
///   overridable via the capability override. Since census C4-C5 (above), this is no longer every
///   `Role::Infix` drop, nor every drop hidden behind a non-safe allomorph 0 — only a rule whose
///   dropping allomorph(s), checked individually, classify `Role::Reduplication`/
///   `Role::CircumfixSuffix` themselves still lands here, regardless of what any OTHER allomorph of
///   the same rule classifies as.
///
/// # Provenance
/// `EvidenceProvenance::Structural`: `structural_composite_attempted` reads directly-inspectable
/// `model.rs` data via `crate::emit::is_structural_rule` (the SAME structural fact the real compile
/// path itself branches on to decide whether to build a structural composite for this rule at all),
/// no oracle witnesses needed to derive the VERDICT itself — the SUPPORTED case's own safe-recall
/// argument (exact containment, not merely a safe superset) was separately, empirically verified
/// against `pg_parse::Morpher` (this crate's own containment fixture), the same "oracle verified the
/// construction, the predicate reads structure" split every other `*FaithfulPredicate` in this
/// module already draws.
///
/// # Node applicability
/// Grammar-wide, not node-specific — same shape `MultiTableFaithfulThreadingPredicate`'s own doc
/// describes: `CircumfixOutputAction` has no corresponding `crate::plan::PlanNodeKind` in today's
/// `enumerate_default` shape at all (this module's own `compose_envelope` doc, "Judgment call:
/// constructs with no distinct plan node"), so `evaluate` ignores `plan_node` entirely and returns
/// the SAME verdict at every node the walk visits — safe under `meet` for the identical reason that
/// doc gives.
pub struct CircumfixStructuralCompositePredicate;

impl CapabilityPredicate for CircumfixStructuralCompositePredicate {
    fn id(&self) -> PredicateId {
        "circumfix-output-action.faithful-structural-composite"
    }

    fn shape_key(&self) -> &'static str {
        "late-structural-reachability"
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::CircumfixOutputAction]
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        _grammar: &Grammar,
        profile: &CharacteristicsProfile,
        _plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        let mut any_observed = false;
        for detail in profile.circumfix_output_action_details() {
            any_observed = true;
            if !detail.structural_composite_attempted {
                return PredicateVerdict::Refuse(CapabilityDiagnostic {
                    predicate: self.id(),
                    construct: format!(
                        "mrule {} allomorph #{} (LHS-material-dropping output action)",
                        detail.rule.0, detail.allomorph_index
                    ),
                    witness:
                        "no allomorph of this allomorph's own rule classifies as crate::emit::\
                              Role::CircumfixPrefix (crate::emit::classify_affix, scanned over \
                              every allomorph), no allomorph -- scanned individually, not just \
                              the first -- classifies as crate::emit::Role::None/Prefix/Suffix/\
                              Infix with LHS-material-dropping content of its own either, and the \
                              rule does not use OutputAction::Modify/InsertContext, so crate::emit\
                              ::is_structural_rule never routes it through the faithful \
                              build_structural_composites construction -- the real compiler \
                              already honestly skips (reports uncovered) this exact allomorph \
                              rather than silently mis-compiling it"
                            .to_string(),
                });
            }
        }
        if any_observed {
            PredicateVerdict::ConfirmOnly
        } else {
            PredicateVerdict::Admit
        }
    }
}

// ---- Reduplication: the config-predicate `cover-template-truncation-reduplication` registers ----

/// The capability predicate for `Reduplication`: every truly reduplicating allomorph must have an
/// actual proposal route. Statically proven prefix-copy, suffix-copy, and one-separator shapes use
/// `crate::peel::ReduplicationPeeler`; other layouts use the structural composite route, which
/// replays `pg_rules::morph::synthesize`. Both remain
/// confirm-only proposers whose candidates are checked by the full morphological engine.
///
/// # Disposition
/// - **Not observed at all** (no allomorph truly reduplicates anywhere in the grammar): vacuously
///   `Admit` — nothing for this predicate to say (mirrors every other `*Predicate` in this
///   registry's own "not applicable here" convention).
/// - **Every observed occurrence has either a peel or structural route**:
///   `PredicateVerdict::ConfirmOnly` — the selected construction is a faithful proposer
///   (`tests/f6_reduplication_peel_chain_depth.rs`'s own containment fixture proves oracle-exact
///   CONTAINMENT against `pg_parse::Morpher` for a real, previously-zero-coverage full-stem
///   reduplication construct — `machine/conformance/languages/
///   suffixing-extension-slot-ordering`'s `mrRedup`, "kimbiakimbia"), but no PROVEN
///   no-false-negative admission-filter argument exists — confirm-only-by-default, the same
///   landing spot every other `ConfigPredicate` characteristic in this registry already uses.
/// - **At least one observed occurrence has neither route**: `PredicateVerdict::Refuse`. A simple
///   true-reduplicating `RealizationalRule` is the principal example: the peeler intentionally only
///   owns `AffixProcessRule`, while this shape does not trigger structural synthesis. The grammar
///   must be refused rather than silently missing recall.
///
/// # Route ownership
/// `peel_attempted` is computed from the same shared rule-wide predicate the runtime peeler uses.
/// `structural_composite_attempted` is computed from `crate::emit::is_structural_rule`, the same
/// predicate the compiler uses. A mixed rule is therefore owned as a unit: if one reduplicating
/// alternative needs structural synthesis, the peeler relinquishes all of the rule's alternatives.
///
/// # Deep/nested reduplication chains stay a SEPARATE, cost (not capability), concern
/// `crate::peel::ReduplicationPeeler`'s nested-reduplication recursion (its own module doc, "Chain
/// depth and nested reduplication") is bounded by the
/// configured `chain_depth_cap` dimension, not by this predicate: a
/// deep chain that exceeds a CONFIGURED cap is a per-word, cost-uncertain runtime refusal
/// (`crate::compose_budget::ComposeError::ChainDepthExceeded`), never a compile-time
/// supported/unsupported capability verdict — capability is proven a-priori and hard-fails; cost is
/// cost-uncertain and only warns/refuses at apply-time under the runtime counter. This predicate's
/// own verdict is therefore identical regardless of how deep any given grammar's reduplication
/// chains happen to run.
///
/// # Provenance
/// `EvidenceProvenance::Structural`: both route facts read the same structural predicates used by
/// the real proposal paths, with no inferred surface heuristic in this evaluator. The SUPPORTED
/// case's own safe-recall argument (oracle-exact containment, not merely a safe superset) was
/// separately, empirically verified against `pg_parse::Morpher`
/// (`tests/f6_reduplication_peel_chain_depth.rs`), the same "oracle verified the construction, the
/// predicate reads structure" split every other `*Predicate` in this module already draws.
///
/// # Node applicability
/// Grammar-wide, not node-specific, like `CircumfixStructuralCompositePredicate`'s own doc
/// describes: `Reduplication` has no corresponding `PlanNodeKind` in today's `enumerate_default`
/// shape (peeling happens entirely OUTSIDE the compiled FST, so there is no plan node to address it
/// by at all) — `evaluate` ignores `plan_node` entirely and returns the SAME verdict at every node
/// the walk visits, safe under `meet` for the identical reason that doc gives.
pub struct ReduplicationPeelSupportedPredicate;

impl CapabilityPredicate for ReduplicationPeelSupportedPredicate {
    fn id(&self) -> PredicateId {
        "reduplication.peel-eligible-rule-kind"
    }

    fn shape_key(&self) -> &'static str {
        "nonregular-process-morphology"
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::Reduplication]
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        _grammar: &Grammar,
        profile: &CharacteristicsProfile,
        _plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        let mut any_observed = false;
        for detail in profile.reduplication_details() {
            any_observed = true;
            if !detail.peel_attempted && !detail.structural_composite_attempted {
                return PredicateVerdict::Refuse(CapabilityDiagnostic {
                    predicate: self.id(),
                    construct: format!(
                        "mrule {} allomorph #{} (true reduplication has no proposal route)",
                        detail.rule.0, detail.allomorph_index
                    ),
                    witness: "the shared rule-wide peel predicate declined this rule and \
                              emit::is_structural_rule did not route it through full-engine \
                              structural synthesis; accepting it would silently lose analyses"
                        .to_string(),
                });
            }
        }
        if any_observed {
            PredicateVerdict::ConfirmOnly
        } else {
            PredicateVerdict::Admit
        }
    }
}

// ---- Compounding: the config-predicate `cover-compounding` registers ----

/// The capability predicate for `Compounding`: splits
/// `CharacteristicKind::Compounding` at CONFIGURATION-PREDICATE granularity (never a blanket
/// variant claim). Originally split `compounding.non-recursive`/`compounding.recursive`
/// into two DIFFERENT verdicts, keyed by `CompoundingDetail::recursive` (`compounding_recursive`'s
/// rule-graph reachability pass — the first predicate here whose input is a GRAPH property of
/// `Grammar.mrules`, not a per-rule/per-subrule check).
///
/// # The recursive split is now closed too — no split remains
/// Closing the recursive split required three things: (1) bound the self-feeding depth; (2) a
/// depth-budgeted faithful cross-product construction; (3) a no-false-negative containment proof.
/// (1) closed first (`CompoundingDetail::max_depth`/`compounding_max_depth` — always finite,
/// no "genuinely unboundable" shape exists for `Compounding`, unlike
/// `CharacteristicKind::QuantifierPattern`'s real Kleene case). (2)/(3) close via `crate::emit`'s
/// "bounded compound loop" (module doc), which now unrolls `max_depth - 1` extra (non-head) root
/// LEVELS — not hardcoded to exactly one — reusing the SAME license-gated non-head root set at
/// every level (`crate::emit::compound_license`, no new precision, only depth), and consumes THIS
/// predicate's own precomputed `max_depth` bound directly (one source of truth: the construction
/// never re-derives it). Containment (propose ⊇ confirm, non-vacuously) is checked against
/// `pg_parse::Morpher::with_max_stem_count` raised past its hardcoded default
/// (`tests/cover_compounding_recursive_depth_bound.rs`'s own containment test). The over-counting
/// direction `compounding_max_depth`'s own doc already establishes ("this over-counts, never
/// under-counts") means the construction's unrolled depth is always AT LEAST the grammar's real
/// achievable depth — the safe direction for an over-approximating proposer.
///
/// # Disposition
/// - **Not observed at all** (no `Compounding` rule in the grammar): vacuously `Admit` — nothing
///   for this predicate to say (mirrors `ReduplicationPeelSupportedPredicate`'s own convention).
/// - **At least one `Compounding` rule observed, recursive or not**: `PredicateVerdict::ConfirmOnly`
///   UNCONDITIONALLY — no further split. `crate::emit::compound_license`'s license-gated head/
///   non-head cross product, now depth-budgeted (`crate::emit`'s "bounded compound
///   loop"/`build_compound_chain`), is a genuinely faithful, over-approximating proposal for EVERY
///   observed configuration, recursive or not (a `Gate`/`Compose`/`Union` shape authored directly
///   against this crate's lexc emitter rather than a real `crate::plan::PlanNodeKind::Gate` node,
///   since this crate does not wire its emitters to the reified `Plan` yet). No proven
///   no-false-negative admission-filter argument exists either way, so `ConfirmOnly` is the
///   correct, permanent landing spot — the same shape `MprGroupAppendNonNarrowingPredicate`'s own
///   doc draws for a kind with no further split ("every observation reaches the SAME verdict").
///   Not `Admit`: promoting an already-`ConfirmOnly` construction further is out of scope here —
///   only `SimultaneousRewrite`'s non-overlap split has reached `Admit` today.
///
/// # Cost stays a SEPARATE, per-grammar concern — never this predicate's own verdict
/// `max_depth` is always finite but never guaranteed SMALL: `CompoundingRuleDef::max_apps` is a bare
/// `u16` with no clamp enforced anywhere in this crate's own loader, so a grammar author could set
/// `multipleApplication` far beyond the DTD's practical ceiling (9). A pathologically deep grammar
/// is a COST/resource-ceiling concern owned by `crate::emit` (see its compound-chain depth budget's
/// own doc for the compile-time refusal it makes), never a capability-layer one — exactly mirroring
/// how `UnorderedOrderingUnionPredicate` stays `ConfirmOnly`. Unlike a capability predicate that
/// closes a construction gap,
/// `Compounding` was provable precisely because its classifying signal (`detail.recursive`) was a
/// CONSTRUCTION gap, not a cost one — so once the construction exists, nothing about `Compounding`
/// licenses a capability-layer cost carve-out. The
/// capability override remains available for any grammar this predicate's own `ConfirmOnly` verdict
/// does not by itself unblock (e.g. a grammar tripping `crate::emit`'s own compile-time budget).
///
/// # Node applicability
/// Like `ReduplicationPeelSupportedPredicate`/`CircumfixStructuralCompositePredicate`:
/// `Compounding` has no corresponding `crate::plan::PlanNodeKind` in today's `enumerate_default`
/// shape (the license-gated cross product is built directly into `crate::emit`'s lexc sections, not
/// a reified `Plan` node) — `evaluate` ignores `plan_node` entirely and scans
/// `CharacteristicsProfile::compounding_details` instead, safe under `meet` for the identical
/// reason those two predicates' own docs give (every node the walk visits gets the SAME verdict).
///
/// # Provenance
/// `EvidenceProvenance::Structural`: both the recursion reachability pass and the
/// license-gate (un)group-awareness contract read directly-inspectable `model.rs`/`Grammar`
/// structure — no oracle witness is needed to derive the verdict itself (the oracle witnesses this
/// change ships separately prove the license-gated PROPOSAL is a correct over-approximation, which
/// is a different claim from what this predicate decides).
pub struct CompoundingRecursionSafePredicate;

impl CapabilityPredicate for CompoundingRecursionSafePredicate {
    fn id(&self) -> PredicateId {
        "compounding.non-recursive"
    }

    fn shape_key(&self) -> &'static str {
        "repeated-application"
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::Compounding]
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        _grammar: &Grammar,
        profile: &CharacteristicsProfile,
        _plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        // Every observed `Compounding` rule gets the same faithful proposal; cost is refused separately at compile time by `DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET`.
        profile
            .compounding_details()
            .next()
            .map_or(PredicateVerdict::Admit, |_| PredicateVerdict::ConfirmOnly)
    }
}

// ---- UnorderedMorphRuleApplication: the config-predicate `cover-unordered-morph-rules` registers ----

/// The capability predicate for `UnorderedMorphRuleApplication`: an observed unordered stratum
/// uses the existing order-union proposal and remains `ConfirmOnly`; no rule-count threshold
/// changes that semantic verdict.
///
/// # Disposition
/// - **Not observed at all** (no `Unordered` stratum in the grammar): vacuously `Admit` — nothing
///   for this predicate to say (mirrors `CompoundingRecursionSafePredicate`'s own convention).
/// - **`unordered-application.chain-depth-bounded`** (an observed `Unordered` stratum):
///   `PredicateVerdict::ConfirmOnly` — `crate::emit::
///   build_deriv_chain`'s existing derivation-layer construction (`crate::unordered`'s own module
///   doc: "the ordering-union proposal IS an existing mechanism") is a
///   genuinely faithful, over-approximating FST proposal for this case, oracle-contained against
///   `pg_parse::Morpher` (`tests/cover_unordered_morph_rules.rs`) — but no proven
///   no-false-negative admission-filter argument exists, so the
///   resting disposition is the same `ConfigPredicate` landing spot every other construct
///   in this file uses.
///
/// # Node applicability
/// Like `ReduplicationPeelSupportedPredicate`/`CompoundingRecursionSafePredicate`:
/// `UnorderedMorphRuleApplication` has no corresponding `crate::plan::PlanNodeKind` in today's
/// `enumerate_default` shape (`build_deriv_chain` is authored directly against this crate's lexc
/// emitter, same as the compounding license-gate — `crate::unordered`'s own module doc) —
/// `evaluate` ignores `plan_node` entirely and scans
/// `CharacteristicsProfile::unordered_stratum_details` instead, safe under `meet` for the
/// identical reason those two predicates' own docs give (every node the walk visits gets the SAME
/// verdict).
///
/// # Provenance
/// `EvidenceProvenance::Structural`: the predicate reads directly-inspectable `model.rs`/`Grammar`
/// structure; the oracle witnesses prove the ordering-union proposal is a correct
/// over-approximation, which is a different claim from what this predicate decides.
pub struct UnorderedOrderingUnionPredicate;

impl CapabilityPredicate for UnorderedOrderingUnionPredicate {
    fn id(&self) -> PredicateId {
        "unordered-application.chain-depth-bounded"
    }

    fn shape_key(&self) -> &'static str {
        "unordered-interactions"
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::UnorderedMorphRuleApplication]
    }

    // The budget bounds `emit::build_deriv_chain`; `uflexc`'s self-looping chains build no such layers.
    fn constrains_strategies(&self) -> &[EmissionStrategy] {
        DERIVATION_LAYER_STRATEGIES
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        _grammar: &Grammar,
        profile: &CharacteristicsProfile,
        _plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        if profile.unordered_stratum_details().next().is_some() {
            PredicateVerdict::ConfirmOnly
        } else {
            PredicateVerdict::Admit
        }
    }
}

// ---- MprGroupAppend / MprGroupOverwrite: the config-predicates `cover-mpr-groups` registers ----

/// The capability predicate for `MprGroupAppend`: the
/// NON-TRACKING baseline for `MprGroupOutput::Append` groups. The split is drawn
/// on `MprGroupOutput`, not on `MprGroup` wholesale — this predicate discharges ONLY
/// `CharacteristicKind::MprGroupAppend`; `Overwrite` is `MprGroupOverwritePredicate`'s
/// own, separately-argued predicate, never inferred from this one.
///
/// # The baseline this predicate verifies
/// Checked here, not merely asserted: NEITHER of this crate's own MPR-consuming propose code paths
/// ever tracks accumulated MPR-group state to reject a candidate mid-derivation —
/// - `crate::gate`'s static root-entry partition (`entry_gate_key`) keys ONLY on
///   `LexEntryDef::mpr` — a candidate's OWN declared MPR set, fixed at grammar-load time, never an
///   accumulated derivation-chain value (that module's own doc, "root-only" caveat: `out_mpr` "is
///   not threaded into the partition key — every group's affix chains are shared, unfiltered",
///   named there as the exact gap this predicate answers);
/// - the ordinary morphological affix-allomorph emitter (`crate::emit::emit_rule_allomorphs`, called
///   from `crate::emit::build_deriv_chain` — the SAME derivation-layer construction
///   `UnorderedOrderingUnionPredicate`'s own doc cites) never reads `AffixAllomorphDef::
///   required_mpr`/`excluded_mpr`/`out_mpr` at all: every allomorph is offered unconditionally,
///   gated only by RHS emittability and `Role` classification (confirmed by inspection of that
///   function's own body, not merely by absence of a grep hit).
///
/// Both propose exactly the license-free superset every `required_mpr`/`excluded_mpr` gate
/// downstream of an `out_mpr`-bearing allomorph would otherwise need dynamic state for, and leave
/// the exact `mpr_group_ok`/`mpr_add_output` fold entirely to confirm (`pg_rules::morph.rs:
/// 1596,1822,2842,2162,3073-3074`) — the same "unfiltered" fallback `crate::gate`'s own module doc
/// names for the one partial code path (root-only MPR/POS gating) that exists there. This is a
/// required verification, not a restatement of "`ConfirmOnly` is safe in principle" (trivially
/// true for ANY non-narrowing baseline): it is the positive proof that THIS
/// crate's actual propose code never accidentally crosses from that baseline into a narrowing
/// filter. Oracle-contained (over-propose, exact-confirm) by
/// `tests/cover_mpr_groups.rs`.
///
/// # Disposition
/// - **Not observed at all**: vacuously `Admit` — nothing for this predicate to say (mirrors
///   `CompoundingRecursionSafePredicate`/`UnorderedOrderingUnionPredicate`'s own convention).
/// - **At least one `Append`-output `MprGroup` observed**: `PredicateVerdict::ConfirmOnly`,
///   UNCONDITIONALLY. Unlike `compounding.non-recursive`/`unordered-application.chain-depth-bounded`,
///   there is no FURTHER split within `Append`: the non-narrowing baseline is safe for every
///   `Append`-output group by construction (the "propose the superset, confirm applies the exact
///   fold" argument does not depend on any per-group structural fact the way recursion-reachability
///   or a stratum's own rule count does), so there is no "`mpr-group.append-output`-vs-something-
///   worse" case to discriminate — every observation reaches the SAME verdict.
///   `PredicateVerdict::Admit` (an accumulated-state ADMISSION FILTER, a materially harder
///   claim) is a separate, unproven step this predicate does NOT make — it only ever proves the
///   safe baseline, never promotes past it.
///
/// # Node applicability
/// Like `CompoundingRecursionSafePredicate`/`UnorderedOrderingUnionPredicate`: `MprGroupAppend`
/// has no corresponding `crate::plan::PlanNodeKind` in today's `enumerate_default` shape — a
/// derivation-state-dependent `Gate` *position*, distinct from today's
/// root-static one, does not exist in this crate at all yet;
/// today's only `Gate` shape (`crate::gate`'s root-static partition) is unconditionally safe and
/// needs no predicate to say so. `evaluate` ignores `plan_node` entirely and scans
/// `CharacteristicsProfile::observations` for an `MprGroupAppend` occurrence instead, safe under
/// `meet` for the identical reason those two predicates' own docs give (every node the walk visits
/// gets the SAME verdict).
///
/// # Big-O + runtime-feature declaration
/// Zero marginal cost: this predicate discharges an EXISTING code path verbatim (`crate::gate`'s
/// partition, `crate::emit::build_deriv_chain`/`emit_rule_allomorphs`, `crate::uflexc`'s lexc
/// construction) — no new FST states/arcs, no new compile-time pass, nothing to calibrate a resource
/// threshold against. `evaluate` itself is `O(#observations)`, a single linear scan, same as every
/// other profile-wide predicate in this file. The required-runtime-feature set is EMPTY: the
/// non-tracking baseline changes nothing about what propose already emits, so there is no query-time
/// operation to declare (unlike `crate::peel::RUNTIME_FEATURE_REDUPLICATION_PEEL`'s per-word peel
/// op) — confirmed, not assumed, since the whole baseline argument is that it adds no new mechanism.
///
/// # Provenance
/// `EvidenceProvenance::Structural`: the claim rests on directly-inspectable `crate::gate`/
/// `crate::emit`/`crate::uflexc` source (which fields those modules read, or don't), not a black-box
/// oracle witness — the oracle witnesses this change ships separately (`tests/cover_mpr_groups.rs`)
/// prove the resulting PROPOSAL is a correct over-approximation against `pg_parse::Morpher`, a
/// different, complementary claim from what this predicate decides.
pub struct MprGroupAppendNonNarrowingPredicate;

impl CapabilityPredicate for MprGroupAppendNonNarrowingPredicate {
    fn id(&self) -> PredicateId {
        "mpr-group.append-output"
    }

    fn shape_key(&self) -> &'static str {
        "nonregular-process-morphology"
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::MprGroupAppend]
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        _grammar: &Grammar,
        profile: &CharacteristicsProfile,
        _plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        let observed = profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::MprGroupAppend);
        if observed {
            PredicateVerdict::ConfirmOnly
        } else {
            PredicateVerdict::Admit
        }
    }
}

/// The capability predicate for `MprGroupOverwrite`: the non-narrowing superset baseline
/// `MprGroupAppendNonNarrowingPredicate` verifies for `Append` holds for `Overwrite` too, so an
/// observed `Overwrite` group rests at `PredicateVerdict::ConfirmOnly` and an absent one is
/// vacuously `PredicateVerdict::Admit`.
///
/// # Why this can never be promoted to `Admit`
/// A monotone-accumulation admission-filter argument — the basis for `mpr-group.append-output`'s
/// own eventual `Admit` candidacy — is UNSOUND BY CONSTRUCTION here.
/// `pg_grammar::model::mpr_add_output`'s own doc: a later rule application can retract exactly
/// the feature an earlier one added, so the accumulated set at any derivation point depends on the
/// SEQUENCE, not just the multiset, of prior outputs. An FST filter sees a single transition and
/// cannot reconstruct that history, so it would silently omit history-dependent analyses. Never add
/// an admission filter on this construct's behalf; confirm is the only sound place to enforce it.
///
/// # Node applicability
/// Same "no corresponding `crate::plan::PlanNodeKind`" shape
/// `MprGroupAppendNonNarrowingPredicate`'s own doc describes — `evaluate` ignores `plan_node` and
/// scans observations directly.
///
/// # Provenance
/// `EvidenceProvenance::Structural`: `MprGroupOutput::Overwrite` is directly-inspectable
/// `model.rs` structure (`characterize`'s own `MprGroupOutput::Overwrite` match arm).
pub struct MprGroupOverwritePredicate;

impl CapabilityPredicate for MprGroupOverwritePredicate {
    fn id(&self) -> PredicateId {
        "mpr-group.overwrite-output"
    }

    fn shape_key(&self) -> &'static str {
        "nonregular-process-morphology"
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::MprGroupOverwrite]
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        _grammar: &Grammar,
        profile: &CharacteristicsProfile,
        _plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        if profile
            .observations()
            .iter()
            .any(|obs| obs.kind == CharacteristicKind::MprGroupOverwrite)
        {
            PredicateVerdict::ConfirmOnly
        } else {
            PredicateVerdict::Admit
        }
    }
}

// ---- QuantifierPattern: the config-predicate `compile-bounded-fst-quantifiers` registers ----

/// The capability predicate for `QuantifierPattern`, covering both bounded and the genuinely
/// unbounded case: a
/// `PatternNode::Quantifier` occurrence is now faithfully COMPILABLE (`crate::replace::Slot::Repeat`,
/// whose `max: Option<u32>` renders EITHER foma's native `^{min,max}` bounded-repetition operator or
/// its native `*`/`^>N` unbounded-repetition operator, `crate::lower::render_slots`'s own doc)
/// PROVIDED the rule's whole pattern shape is otherwise one `crate::replace::compile_rewrite_rule_
/// subset` actually attempts (`QuantifierPatternDetail::compile_attempted`) — `all_bounded` is no
/// longer, by itself, a disposition-driving fact (see "Disposition" below); it stays on
/// `QuantifierPatternDetail` purely as informational structural evidence (consumed by
/// `crate::characterization`'s own cost-uncertainty health finding for an unbounded rule, NOT by this
/// predicate).
///
/// # Disposition
/// - **Not observed to use `Quantifier` at all**: vacuously `Admit` — nothing for this predicate to
///   say (mirrors `RightToLeftRewriteFaithfulReversalPredicate`'s own "not applicable here"
///   convention).
/// - **The rule's whole pattern shape compiles** (`compile_attempted`, REGARDLESS of `all_bounded`):
///   `PredicateVerdict::ConfirmOnly` — bounded OR unbounded native-operator expansion is a
///   genuinely faithful FST construction for the SUPPORTED case (this change's own containment
///   fixtures, `tests/phase_c_quantifier.rs`, prove oracle-exact equality for a quantifier used in
///   an ENVIRONMENT, both bounded and unbounded — see that module's own doc for why a LHS/RHS-
///   focus-quantified rule's full containment against `pg_rules::rewrite` is a SEPARATE,
///   documented, pre-existing confirm-engine gap this change surfaces but does not fix,
///   `crate::replace` module doc's "Confirm-engine finding"), but no PROVEN no-false-negative
///   admission-filter argument exists for the construct in general
///   — so this is confirm-only-by-default, the same landing spot
///   `RightToLeftRewriteFaithfulReversalPredicate`/`MultiTableFaithfulThreadingPredicate`
///   already use. **A genuinely unbounded quantifier (`!all_bounded`) is no longer, by itself, a
///   reason to withhold this** — the ORIGINAL version of this predicate `Refuse`d unconditionally
///   whenever `!all_bounded`, because the real compiler used to bail (`None`) on every unbounded
///   quantifier regardless of shape; now that `pattern_slots` actually accepts a well-formed
///   unbounded quantifier, refusing it here too would just be a SECOND, redundant conservative
///   check the real compiler's own `compile_attempted` fact already supersedes.
/// - **The rule's pattern shape does not compile at all** (`!compile_attempted` — an ambiguous
///   disagree-polarity alpha var, an inverted or
///   alpha-nested or empty-children quantifier elsewhere in the rule's own patterns, or an
///   unresolvable owning table): `PredicateVerdict::Refuse` — this predicate never claims more
///   than the real compiler actually attempts.
///
/// # Provenance
/// `EvidenceProvenance::Structural`: `compile_attempted` reads directly-inspectable `model.rs`
/// data (no oracle witnesses needed to derive the verdict itself) — the SUPPORTED case's own
/// safe-recall argument was separately, empirically verified for the environment-quantifier shape,
/// both bounded and unbounded (`tests/phase_c_quantifier.rs`'s own containment fixtures), the same
/// "oracle verified the construction, the predicate reads structure" split
/// `MultiTableFaithfulThreadingPredicate`'s own doc draws.
///
/// # Node applicability
/// Like `SimultaneousSubruleOverlapPredicate`/`RightToLeftRewriteFaithfulReversalPredicate`,
/// addressed via `rewrite_rule_of` at a rewrite-rule leaf node — the SAME plan-node-extraction
/// helper, reused rather than re-derived.
pub struct QuantifierBoundedExpansionPredicate;

impl CapabilityPredicate for QuantifierBoundedExpansionPredicate {
    fn id(&self) -> PredicateId {
        "quantifier.bounded-expansion"
    }

    fn shape_key(&self) -> &'static str {
        "repeated-application"
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::QuantifierPattern]
    }

    // `compile_attempted` reads `pattern_slots`, a cascade seam the mainline emitter never asks.
    fn constrains_strategies(&self) -> &[EmissionStrategy] {
        CASCADE_COMPOSING_STRATEGIES
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        _grammar: &Grammar,
        profile: &CharacteristicsProfile,
        plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        let Some(rule) = rewrite_rule_of(plan_node) else {
            return PredicateVerdict::Admit;
        };
        let Some(detail) = profile.quantifier_detail(rule) else {
            // Not observed to use Quantifier at all -- nothing for this predicate to say (doc).
            return PredicateVerdict::Admit;
        };
        // `all_bounded` is not consulted: an unbounded quantifier alone is not a reason to Refuse; `compile_attempted` is the only gate.
        if !detail.compile_attempted {
            return PredicateVerdict::Refuse(CapabilityDiagnostic {
                predicate: self.id(),
                construct: format!("prule {} (Quantifier/OptionalSegmentSequence)", rule.0),
                witness: "some LHS/RHS/environment construct this rule's own patterns use -- \
                          an ambiguous disagree-polarity alpha var, an inverted (min > max, \
                          both concrete), alpha-nested, or empty-children quantifier, or an \
                          unresolvable owning character-definition \
                          table -- blocks crate::replace::pattern_slots from accepting this rule's \
                          whole pattern shape at all. (A GENUINELY unbounded quantifier, max=-1, is \
                          NOT by itself such a construct: crate::replace::Slot::Repeat's \
                          max: Option<u32> widening compiles it via foma's own native E*/E^>N xre \
                          operator.)"
                    .to_string(),
            });
        }
        PredicateVerdict::ConfirmOnly
    }
}

// ---- Epenthesis ----

/// The capability predicate for `Epenthesis`. `CharacteristicKind::Epenthesis`'s own trigger
/// (`RewriteRuleDef::lhs.nodes.is_empty()`, `characterize`'s own comment on model.rs:417's "empty
/// pattern if absent (epenthesis rules)" convention) is, on inspection, ALREADY handled faithfully
/// by mechanisms this crate ships for an unrelated reason — this predicate documents and verifies
/// that fact rather than fixing a narrowing bug, the same "was already at the safe baseline"
/// shape `MprGroupAppendNonNarrowingPredicate`'s own doc describes.
///
/// # The two-sided evidence
/// - **PROPOSE side** (`crate::emit`): `crate::emit::probe_would_refuse` is `true` the instant
///   ANY `PhonRuleDef::Rewrite` rule in the grammar has an empty LHS — EXACTLY
///   `CharacteristicKind::Epenthesis`'s own trigger, checked unconditionally over every rule in
///   `g.prules` regardless of whether the specific rule being asked about fires for any given
///   word (that function's own doc). Whenever this fires, [`crate::emit::structural_candidate_
///   rules`] widens to cover every ordinary `Role::Prefix`/`Role::Suffix`/`Role::Infix` morph rule
///   in the WHOLE grammar (not just ones that themselves drop LHS material,
///   `crate::emit::is_structural_rule`'s own narrower test) — `crate::preexpand`'s ordinary
///   fusion/interdigitation probe cannot represent them correctly either (its own probe,
///   `pg_rules::surface_probe::probe_synthesize`, refuses for every candidate in the affected
///   stratum), so `crate::emit`'s module doc names `crate::emit::build_structural_composites`
///   as "their only remaining path to a phonology-resolved surface." That mechanism resynthesizes
///   every candidate surface via the REAL morphological engine
///   (`pg_rules::morph::synthesize`/`crate::emit::probe_surface`/`Morpher::generate_words`),
///   never a literal-text splice or an FST regex approximation of the empty-LHS rule itself — so
///   it is faithful for whatever epenthesis environment/RHS shape the grammar actually declares,
///   `PatternNode` variety notwithstanding (unlike `RightToLeftRewriteFaithfulReversalPredicate`/
///   `MetathesisFaithfulSwapPredicate`, there is no narrower `crate::replace::pattern_slots`
///   admission floor to check here at all: this construct's own faithful path never asks that
///   question in the first place). This is unconditional on the rule's mere existence — there is
///   no narrower shape within "epenthesis" for propose to fall short on.
/// - **CONFIRM side** (`pg_rules::rewrite`): `syn_epenthesis`/`ana_epenthesis` (the oracle
///   `pg_parse::Morpher` itself calls through its own stratum cascade) were freshly re-investigated
///   for this predicate. `ana_epenthesis`'s own doc comment records that a previously-suspected
///   oracle gap (`tests/phase_c_right_to_left.rs`'s "Morpher finds no analysis for ANY word of an
///   epenthesis fixture") could NOT be reproduced against the code as it exists today, and
///   `pg-rules/tests/rewrite_gate.rs::epenthesis_natural_class_rhs_round_trips_with_environment`
///   (added alongside that investigation) pins the correct round-trip: an environment-gated,
///   natural-class-RHS epenthesis rule synthesizes the obligatory insertion AND recovers the
///   pre-insertion analysis (the inserted segment marked `Optional`, never deleted) in BOTH
///   `LeftToRight` and `RightToLeft` iteration order.
///
/// # Disposition
/// - **Not observed at all**: vacuously `PredicateVerdict::Admit` — mirrors every other predicate
///   in this file's own convention.
/// - **At least one `Epenthesis` occurrence observed**: `PredicateVerdict::ConfirmOnly`,
///   UNCONDITIONALLY — every observation reaches the SAME verdict, the same "no
///   `something`-vs-something-worse case to discriminate" shape
///   `MprGroupAppendNonNarrowingPredicate`'s own doc describes: `probe_would_refuse`'s trigger
///   IS this characteristic's own trigger (not a narrower sub-condition of it), so there is no
///   in-scope/out-of-scope pattern-shape split the way `RightToLeftRewriteFaithfulReversalPredicate`/
///   `MetathesisFaithfulSwapPredicate`/`QuantifierBoundedExpansionPredicate` each have — this
///   predicate's own containment test
///   (`tests/epenthesis_structural_route_containment.rs`) built a synthetic delanguaged grammar
///   exercising exactly this shape (a root + an ordinary `Role::Suffix` rule + an
///   environment-gated epenthesis rule between them) and found candidates ARE over-proposed
///   (`FomaOutcome::candidates_generated > 0` including the raw, un-inserted-into spelling) while
///   confirm prunes to EXACTLY the oracle's own `pg_parse::Morpher` analysis set — no shape was
///   found where containment fails, so no `Refuse` witness exists to carve out. [`PredicateVerdict::
///   Admit`] (an accumulated no-false-negative admission-filter proof) is a
///   separate, unproven step this predicate does NOT make — it only ever proves the safe baseline
///   ConfigPredicate landing spot every other characteristic in this file rests at absent such a
///   proof.
///
/// # Out of scope (documented, not silently ignored)
/// Like `MetathesisDetail::swap_construction_attempted`'s own disclaimer, this predicate does not
/// model a runtime-resource dimension: `crate::emit::build_structural_composites`'s closure-depth
/// resource envelope and the shared
/// runtime resource limits are calibrated separately and are not structural facts about any one
/// epenthesis rule.
///
/// # Node applicability
/// `CharacteristicKind::Epenthesis`'s own `ModelLocation` is a `PhonRuleDef::Rewrite` rule, which
/// (unlike `CircumfixOutputAction`/`Reduplication`/`Compounding`/`MprGroupAppend`/`MprGroupOverwrite`)
/// DOES get its own ordinary `crate::plan::PlanNodeKind::Leaf`
/// (`FragmentSpec::RewriteRule { rule }`, minted unconditionally for every `PRuleId` — the same
/// leaf `RightToLeftRewriteFaithfulReversalPredicate`/`MetathesisFaithfulSwapPredicate` key off
/// via `rewrite_rule_of`). But THIS predicate's own subject matter is not "is this rule's own
/// leaf faithfully compiled" (unlike those two) — it is the GRAMMAR-WIDE side effect the rule's
/// mere presence has on OTHER rules' own propose route entirely (module doc above), which no single
/// leaf address captures. `evaluate` therefore ignores `plan_node` and scans
/// `CharacteristicsProfile::observations` directly instead, the same "grammar-wide, not
/// node-specific" shape `MprGroupAppendNonNarrowingPredicate`/
/// `MprGroupOverwritePredicate`'s own docs describe, for a different underlying reason
/// (those two truly have no corresponding leaf at all; this one has a leaf whose address is simply
/// irrelevant to the question this predicate asks).
///
/// # Provenance
/// `EvidenceProvenance::Structural`: `probe_would_refuse`'s own check is directly-inspectable
/// `model.rs` structure (no oracle witness needed to derive the verdict itself) — the propose-side
/// recall argument (structural composites resynthesize via the real engine) and the confirm-side
/// correctness argument (the fresh `ana_epenthesis`/`syn_epenthesis` round-trip) were both
/// separately, empirically verified (this predicate's own containment test; `pg-rules/tests/
/// rewrite_gate.rs`), the same "oracle verified the construction, the predicate reads structure"
/// split every other `*Predicate` in this module already draws.
pub struct EpenthesisStructuralRoutePredicate;

impl CapabilityPredicate for EpenthesisStructuralRoutePredicate {
    fn id(&self) -> PredicateId {
        "epenthesis.structural-composite-route"
    }

    fn shape_key(&self) -> &'static str {
        "wide-phonology"
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::Epenthesis]
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        _grammar: &Grammar,
        profile: &CharacteristicsProfile,
        _plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        let observed = profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::Epenthesis);
        if observed {
            PredicateVerdict::ConfirmOnly
        } else {
            PredicateVerdict::Admit
        }
    }
}

// ---- The predicate registry (the "no silent vacuous pass" coverage check) ----

/// A collection of `CapabilityPredicate`s, queryable for whether a `CharacteristicKind` is
/// discharged by at least one of them.
#[derive(Default)]
pub struct PredicateRegistry {
    predicates: Vec<Box<dyn CapabilityPredicate>>,
}

impl PredicateRegistry {
    pub fn new() -> Self {
        PredicateRegistry::default()
    }

    pub fn register(&mut self, predicate: Box<dyn CapabilityPredicate>) {
        self.predicates.push(predicate);
    }

    pub fn discharges(&self, kind: CharacteristicKind) -> bool {
        self.predicates
            .iter()
            .any(|p| p.discharges().contains(&kind))
    }

    pub fn predicates(&self) -> &[Box<dyn CapabilityPredicate>] {
        &self.predicates
    }
}

/// The registry this crate ships: twelve REAL predicates
/// (`SimultaneousSubruleOverlapPredicate`, `MultiTableFaithfulThreadingPredicate`,
/// `RightToLeftRewriteFaithfulReversalPredicate`, `QuantifierBoundedExpansionPredicate`,
/// `MetathesisFaithfulSwapPredicate`, `CircumfixStructuralCompositePredicate`,
/// `ReduplicationPeelSupportedPredicate`, `CompoundingRecursionSafePredicate`,
/// `UnorderedOrderingUnionPredicate`, `MprGroupAppendNonNarrowingPredicate`,
/// `MprGroupOverwritePredicate`, `EpenthesisStructuralRoutePredicate`) — proving the coverage
/// contract holds with a real, evidenced proof for every `ConfigPredicate` characteristic this
/// crate's `model.rs` names. Every one of them reads `profile` for real; none is a stub that
/// refuses regardless of what the grammar contains.
pub fn default_registry() -> PredicateRegistry {
    let mut r = PredicateRegistry::new();
    r.register(Box::new(SimultaneousSubruleOverlapPredicate));
    r.register(Box::new(MultiTableFaithfulThreadingPredicate));
    r.register(Box::new(RightToLeftRewriteFaithfulReversalPredicate));
    r.register(Box::new(QuantifierBoundedExpansionPredicate));
    r.register(Box::new(MetathesisFaithfulSwapPredicate));
    r.register(Box::new(CircumfixStructuralCompositePredicate));
    r.register(Box::new(ReduplicationPeelSupportedPredicate));
    r.register(Box::new(CompoundingRecursionSafePredicate));
    r.register(Box::new(UnorderedOrderingUnionPredicate));
    r.register(Box::new(MprGroupAppendNonNarrowingPredicate));
    r.register(Box::new(MprGroupOverwritePredicate));
    r.register(Box::new(EpenthesisStructuralRoutePredicate));
    r
}

/// The "no silent vacuous pass" requirement: every `CharacteristicKind`
/// whose `CharacteristicKind::default_disposition` is `Disposition::ConfigPredicate` must be
/// named by at least one registered predicate's
/// `CapabilityPredicate::discharges`. Returns the undischarged kinds (empty iff `registry` is
/// complete) rather than a bool, so a failing check can report exactly what's missing.
pub fn undischarged_kinds(registry: &PredicateRegistry) -> Vec<CharacteristicKind> {
    CharacteristicKind::ALL
        .iter()
        .copied()
        .filter(|k| k.default_disposition() == Disposition::ConfigPredicate)
        .filter(|k| !registry.discharges(*k))
        .collect()
}

/// The converse of [`undischarged_kinds`]: registered predicates that can never be consulted.
///
/// `compose_envelope` only evaluates predicates for observations whose disposition is not
/// `Disposition::Proven`, so a predicate discharging Proven kinds alone is dead on arrival. That is
/// invisible without this check — such a registration compiles, runs, changes no test and no
/// behaviour, and the only symptom is that the verdict you expected never appears.
pub fn inert_predicates(registry: &PredicateRegistry) -> Vec<PredicateId> {
    registry
        .predicates
        .iter()
        .filter(|predicate| {
            predicate
                .discharges()
                .iter()
                .all(|kind| kind.default_disposition() == Disposition::Proven)
        })
        .map(|predicate| predicate.id())
        .collect()
}

// ---- GrammarWideCheck: whole-grammar, per-strategy facts (not per plan node) ----

/// A grammar-wide capability fact for exactly one `EmissionStrategy`, evaluated once against the
/// whole grammar and plan rather than per `PlanNodeKind`.
///
/// Deliberately a SEPARATE trait from [`CapabilityPredicate`] rather than one `evaluate` shared by
/// both shapes: a predicate answers "is this plan node faithful" and is checked at every node a
/// relevant characteristic touches, while a fact here answers "can this compiler represent this
/// grammar at all" and has no node to be checked at. A shared signature would leave one of
/// `plan_node`/nothing permanently unused by every implementor of one of the two kinds — the
/// "control that cannot act" shape `inert_predicates` exists to catch on the predicate side.
pub trait GrammarWideCheck {
    /// e.g. `"surface-probe.circumfix-zone-exclusive-allomorph"`.
    fn id(&self) -> PredicateId;
    /// The one `EmissionStrategy` this fact's proposer shape is about.
    fn strategy(&self) -> EmissionStrategy;
    /// The `crate::advice_catalog` remedy shape this check's refusal maps to.
    fn shape_key(&self) -> &'static str;
    fn provenance(&self) -> EvidenceProvenance;
    /// `None` when the fact does not apply to `semantics`/`plan`; `Some(Refuse(..))` when it does.
    /// Never `Some(Admit)`/`Some(ConfirmOnly)` — a grammar-wide fact only ever narrows.
    fn evaluate(&self, semantics: &GrammarSemantics<'_>, plan: &Plan) -> Option<CompileDecision>;
}

/// `crate::emit::eager_route_refuses_mixed_circumfix_zone`, published for `TunedSurfaceProbed`.
pub struct TunedSurfaceMixedCircumfixZoneCheck;

impl GrammarWideCheck for TunedSurfaceMixedCircumfixZoneCheck {
    fn id(&self) -> PredicateId {
        "surface-probe.circumfix-zone-exclusive-allomorph"
    }
    fn strategy(&self) -> EmissionStrategy {
        EmissionStrategy::TunedSurfaceProbed
    }
    fn shape_key(&self) -> &'static str {
        "nonregular-process-morphology"
    }
    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }
    fn evaluate(&self, semantics: &GrammarSemantics<'_>, _plan: &Plan) -> Option<CompileDecision> {
        crate::emit::eager_route_refuses_mixed_circumfix_zone(semantics.grammar()).then(|| {
            CompileDecision::Refuse(vec![CapabilityDiagnostic {
                predicate: self.id(),
                construct:
                    "rule mixing a circumfix allomorph with a non-circumfix, non-zero allomorph"
                        .to_string(),
                witness: "EmissionStrategy::TunedSurfaceProbed widens a circumfix-bearing rule \
                          into both derivational zones, but a \
                          Prefix/Suffix/Infix/Reduplication/Process-classified allomorph on that \
                          same rule is reported uncovered by the zone it does not own, and \
                          refuses the build"
                    .to_string(),
            }])
        })
    }
}

/// `crate::emit::templated_route_uncovered_refusal`, published for `TemplatedUnderlyingTokens`.
pub struct TemplatedRouteUncoveredCheck;

impl GrammarWideCheck for TemplatedRouteUncoveredCheck {
    fn id(&self) -> PredicateId {
        "templated-route.emission-uncovered"
    }
    fn strategy(&self) -> EmissionStrategy {
        EmissionStrategy::TemplatedUnderlyingTokens
    }
    fn shape_key(&self) -> &'static str {
        "nonregular-process-morphology"
    }
    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }
    fn evaluate(&self, semantics: &GrammarSemantics<'_>, _plan: &Plan) -> Option<CompileDecision> {
        crate::emit::templated_route_uncovered_refusal(semantics.grammar())
    }
}

/// `crate::capability_gate::rule_cascade_uncompilable_refusal`, published for
/// `TemplatedUnderlyingTokens`.
pub struct RuleCascadeUncompilableCheck;

impl GrammarWideCheck for RuleCascadeUncompilableCheck {
    fn id(&self) -> PredicateId {
        "templated-route.rule-cascade-uncompilable"
    }
    fn strategy(&self) -> EmissionStrategy {
        EmissionStrategy::TemplatedUnderlyingTokens
    }
    fn shape_key(&self) -> &'static str {
        "nonregular-process-morphology"
    }
    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }
    fn evaluate(&self, semantics: &GrammarSemantics<'_>, _plan: &Plan) -> Option<CompileDecision> {
        crate::capability_gate::rule_cascade_uncompilable_refusal(semantics.grammar())
    }
}

/// `templated_shape_floor`, published for `TemplatedUnderlyingTokens` as an ordinary registered
/// check rather than `with_strategy_coverage`'s own hardcoded strategy match.
pub struct TemplatedShapeFloorCheck;

impl GrammarWideCheck for TemplatedShapeFloorCheck {
    fn id(&self) -> PredicateId {
        TEMPLATED_UNSUPPORTED_SHAPE_PREDICATE
    }
    fn strategy(&self) -> EmissionStrategy {
        EmissionStrategy::TemplatedUnderlyingTokens
    }
    fn shape_key(&self) -> &'static str {
        "nonregular-process-morphology"
    }
    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }
    fn evaluate(&self, semantics: &GrammarSemantics<'_>, _plan: &Plan) -> Option<CompileDecision> {
        match templated_shape_floor(semantics) {
            CompileDecision::Admit => None,
            refused => Some(refused),
        }
    }
}

/// `crate::replace::grammar_has_no_tokenizable_root`, published for `PlanComposed`.
pub struct PlanComposedNoTokenizableRootCheck;

impl GrammarWideCheck for PlanComposedNoTokenizableRootCheck {
    fn id(&self) -> PredicateId {
        "strategy-materializer.tokenizable-root-required"
    }
    fn strategy(&self) -> EmissionStrategy {
        EmissionStrategy::PlanComposed
    }
    fn shape_key(&self) -> &'static str {
        "nonregular-process-morphology"
    }
    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }
    fn evaluate(&self, semantics: &GrammarSemantics<'_>, _plan: &Plan) -> Option<CompileDecision> {
        crate::replace::grammar_has_no_tokenizable_root(semantics.grammar()).then(|| {
            CompileDecision::Refuse(vec![CapabilityDiagnostic {
                predicate: self.id(),
                construct: "lexicon with no root allomorph carrying a tokenizable underlying shape"
                    .to_string(),
                witness: "EmissionStrategy::PlanComposed's emit_underlying_filtered skips every \
                          root allomorph that is pattern-only or references a natural class in \
                          its shape, so a lexicon with no other root produces zero entries in \
                          every gated group and build_controllable's net stays empty"
                    .to_string(),
            }])
        })
    }
}

/// `crate::build::unbuildable_markers`, published for `PlanComposed`.
pub struct PlanComposedMarkerCheck;

impl GrammarWideCheck for PlanComposedMarkerCheck {
    fn id(&self) -> PredicateId {
        "strategy-materializer.marker-subtree-not-buildable"
    }
    fn strategy(&self) -> EmissionStrategy {
        EmissionStrategy::PlanComposed
    }
    fn shape_key(&self) -> &'static str {
        crate::advice_catalog::PLAN_COMPOSED_MISSING_SUBTREES_SHAPE_KEY
    }
    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }
    fn evaluate(&self, semantics: &GrammarSemantics<'_>, plan: &Plan) -> Option<CompileDecision> {
        let markers = crate::build::unbuildable_marker_material(plan, semantics.grammar());
        if markers.is_empty() {
            return None;
        }
        Some(CompileDecision::Refuse(
            markers
                .iter()
                .map(|marker| {
                    let marker = format!("{marker:?}");
                    CapabilityDiagnostic {
                        predicate: self.id(),
                        construct: marker.clone(),
                        witness: format!(
                            "EmissionStrategy::PlanComposed uses build_controllable, which skips \
                             the required {marker} subtree; selecting PlanComposed would silently \
                             omit its material"
                        ),
                    }
                })
                .collect(),
        ))
    }
}

/// `crate::emit::eager_route_drops_root_spellings`, published for `TunedSurfaceProbed`.
///
/// Two live witnesses, distinguished by which `VariantLimit` fires. `VariantLimit::BytesExhausted`
/// is unconditional; the `VariantLimit::Unbounded` branch additionally requires a non-empty
/// `allo.environments`, since commit 620863f6 made `collect_roots` route an environment-free
/// unbounded shape to a compiled regex entry instead of dropping it -- retiring the historical
/// `Unbounded`-branch witnesses (`languages/polysynthetic-stratal-derivation-chain`,
/// `edge-cases/backend-strata-generic`, `edge-cases/guesser-pattern-root-fallback` all now compile
/// clean under `TunedSurfaceProbed`; see `tests/pattern_root_regex_route_gate.rs`). The current
/// `Unbounded`-branch witness is `staging:edge-cases/pattern-root-required-environment`, whose
/// pattern root carries a `<RequiredEnvironments>` the regex route cannot honour. Either way the
/// ARTIFACT the eager route would produce is not a faithful proposer for the affected entry —
/// confirmed by `FomaProposer::new` itself refusing to trust it
/// (`envelope_agrees_with_compiler_gate`'s own `the_published_root_spelling_fact_never_over_claims_a_drop`,
/// which asserts exactly that for every fixture this fact fires on). That is not "the fact fired"
/// alone -- it is the compiler's own emission report naming a real spelling it silently could not
/// represent, which is what separates a correct refusal from an over-refusal.
pub struct EagerRouteDropsRootSpellingsCheck;

impl GrammarWideCheck for EagerRouteDropsRootSpellingsCheck {
    fn id(&self) -> PredicateId {
        "surface-probe.root-spelling-cap"
    }
    fn strategy(&self) -> EmissionStrategy {
        EmissionStrategy::TunedSurfaceProbed
    }
    fn shape_key(&self) -> &'static str {
        "repeated-application"
    }
    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }
    fn evaluate(&self, semantics: &GrammarSemantics<'_>, _plan: &Plan) -> Option<CompileDecision> {
        crate::emit::eager_route_drops_root_spellings(semantics.grammar()).then(|| {
            CompileDecision::Refuse(vec![CapabilityDiagnostic {
                predicate: self.id(),
                construct: "root allomorph whose pattern shape cannot be enumerated in full"
                    .to_string(),
                witness: "EmissionStrategy::TunedSurfaceProbed's eager route leaves root spellings \
                          out of the compiled network -- an unbounded `*` shape, which has no finite \
                          spelling set at any budget, or REP_VARIANT_BYTE_BUDGET reached mid \
                          enumeration -- so confirm has no candidate left to recover them. Breadth \
                          alone never reaches here: a complete enumeration is emitted in full \
                          however large"
                    .to_string(),
            }])
        })
    }
}

/// `crate::replace::grammar_has_untokenizable_root_shape`, published for `TemplatedUnderlyingTokens`.
///
/// No fixture in `envelope_agrees_with_compiler_gate`'s sweep exercises this fact (its own
/// `the_published_untokenizable_root_shape_fact_never_over_claims_a_refusal` non-vacuity assertion
/// is satisfied by a synthetic grammar built inside that test, not by a discovered fixture), so
/// wiring it moves no row in the fixture inventory; its effect is confined to the two reference
/// grammars (Aweti, Mbugwe) reported separately under `-Mode corpus-test`.
pub struct UntokenizableRootShapeCheck;

impl GrammarWideCheck for UntokenizableRootShapeCheck {
    fn id(&self) -> PredicateId {
        "templated-route.tokenizable-root-shape"
    }
    fn strategy(&self) -> EmissionStrategy {
        EmissionStrategy::TemplatedUnderlyingTokens
    }
    fn shape_key(&self) -> &'static str {
        "nonregular-process-morphology"
    }
    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }
    fn evaluate(&self, semantics: &GrammarSemantics<'_>, _plan: &Plan) -> Option<CompileDecision> {
        crate::replace::grammar_has_untokenizable_root_shape(semantics.grammar()).then(|| {
            CompileDecision::Refuse(vec![CapabilityDiagnostic {
                predicate: self.id(),
                construct: "root allomorph shape naming an abstract natural-class node".to_string(),
                witness:
                    "EmissionStrategy::TemplatedUnderlyingTokens's SegAlphabet::encode_shape \
                          requires every root allomorph shape to name a concrete character \
                          definition; a root carrying an abstract class-derived node has no single \
                          codepoint to hand it, so the templated route cannot tokenize this lexicon"
                        .to_string(),
            }])
        })
    }
}

/// The registry this crate ships: every grammar-wide fact `crate::backend_selection::select_backends`
/// applies, named and strategy-scoped rather than threaded as a growing positional argument list.
pub fn default_grammar_wide_checks() -> Vec<Box<dyn GrammarWideCheck>> {
    vec![
        Box::new(TunedSurfaceMixedCircumfixZoneCheck),
        Box::new(TemplatedRouteUncoveredCheck),
        Box::new(RuleCascadeUncompilableCheck),
        Box::new(TemplatedShapeFloorCheck),
        Box::new(PlanComposedNoTokenizableRootCheck),
        Box::new(PlanComposedMarkerCheck),
        Box::new(EagerRouteDropsRootSpellingsCheck),
        Box::new(UntokenizableRootShapeCheck),
    ]
}

/// The single check `with_strategy_coverage`'s old hardcoded `TemplatedUnderlyingTokens` branch applied, preserved for every caller that is not `crate::backend_selection`.
fn baseline_grammar_wide_checks() -> Vec<Box<dyn GrammarWideCheck>> {
    vec![Box::new(TemplatedShapeFloorCheck)]
}

/// Everything a compiler's own capability envelope composes from: per-plan-node
/// `CapabilityPredicate`s and per-strategy `GrammarWideCheck`s, bundled so
/// `compose_envelope_across_strategies` takes one argument rather than a growing positional list.
pub struct CapabilityContributions<'a> {
    pub predicates: &'a PredicateRegistry,
    pub grammar_wide: &'a [Box<dyn GrammarWideCheck>],
}

impl<'a> CapabilityContributions<'a> {
    pub fn new(
        predicates: &'a PredicateRegistry,
        grammar_wide: &'a [Box<dyn GrammarWideCheck>],
    ) -> Self {
        Self {
            predicates,
            grammar_wide,
        }
    }
}

// ---- Bottom-up envelope composition + the compile decision ----

/// `compose_envelope`'s overall, whole-plan verdict is plain data the pack format also reads, so
/// it lives in `pg-health`; re-exported here at its historical path. Distinct from
/// `PredicateVerdict` (a PER-PREDICATE, single-node verdict, carrying at most one
/// `CapabilityDiagnostic`): composing a whole plan can collect refusals from many different
/// nodes/observations, and `CompileDecision::Refuse` widens that to a deduplicated `Vec` at
/// exactly the point those per-node/per-observation verdicts get folded together.
pub use pg_health::capability::CompileDecision;

/// The lattice, made explicit: `Refuse` dominates `ConfirmOnly` dominates `Admit`/`Proven` —
/// `meet(a, b)` is this lattice's greatest-lower-bound over the total order `Admit < ConfirmOnly <
/// Refuse`.
///
/// ```text
/// meet(Admit,       Admit)       = Admit
/// meet(Admit,       ConfirmOnly) = ConfirmOnly
/// meet(ConfirmOnly, ConfirmOnly) = ConfirmOnly
/// meet(_,           Refuse(d2))  = Refuse(d1 ++ d2, content-deduplicated)  -- Refuse always wins
/// meet(Refuse(d1),  _)           = Refuse(d1 ++ d2, content-deduplicated)
/// ```
/// Two `Refuse`s meet to a `Refuse` carrying the UNION of both sides' diagnostics, content-
/// deduplicated: the same `CapabilityDiagnostic` can be reached via two different DAG paths to a
/// shared node (content-addressed sharing means a single offending leaf can be a descendant of
/// several parents), and it must not appear twice in the final report merely because it was visited
/// twice.
pub fn meet(a: CompileDecision, b: CompileDecision) -> CompileDecision {
    match (a, b) {
        (CompileDecision::Refuse(mut left), CompileDecision::Refuse(right)) => {
            for diag in right {
                if !left.contains(&diag) {
                    left.push(diag);
                }
            }
            CompileDecision::Refuse(left)
        }
        (CompileDecision::Refuse(d), _) | (_, CompileDecision::Refuse(d)) => {
            CompileDecision::Refuse(d)
        }
        (CompileDecision::ConfirmOnly, _) | (_, CompileDecision::ConfirmOnly) => {
            CompileDecision::ConfirmOnly
        }
        (CompileDecision::Admit, CompileDecision::Admit) => CompileDecision::Admit,
    }
}

/// Widens a single-diagnostic `PredicateVerdict` into a `CompileDecision` so it can be `meet`-folded.
fn verdict_to_decision(verdict: PredicateVerdict) -> CompileDecision {
    match verdict {
        PredicateVerdict::Admit => CompileDecision::Admit,
        PredicateVerdict::ConfirmOnly => CompileDecision::ConfirmOnly,
        PredicateVerdict::Refuse(diag) => CompileDecision::Refuse(vec![diag]),
    }
}

/// The decision floor for an observed, non-`Proven` kind no registered predicate discharges at all.
/// See docs/research/pg-foma-capability-design-notes.md.
fn disposition_floor(disposition: Disposition) -> CompileDecision {
    match disposition {
        Disposition::Proven => CompileDecision::Admit,
        Disposition::ConfigPredicate | Disposition::ConfirmOnly => CompileDecision::ConfirmOnly,
    }
}

/// Computes `node_id`'s bottom-up `CompileDecision`, memoized by `NodeId` so shared DAG nodes are evaluated exactly once.
/// See docs/research/pg-foma-capability-design-notes.md.
fn node_decision(
    grammar: &Grammar,
    plan: &Plan,
    profile: &CharacteristicsProfile,
    predicates: &[&dyn CapabilityPredicate],
    relevant_kinds: &HashSet<CharacteristicKind>,
    node_id: NodeId,
    cache: &mut HashMap<NodeId, CompileDecision>,
) -> CompileDecision {
    if let Some(cached) = cache.get(&node_id) {
        return cached.clone();
    }
    // A dangling id is a caller/plan-construction bug, not a capability judgment; fold in as Admit rather than panic.
    let Some(kind) = plan.get(node_id) else {
        return CompileDecision::Admit;
    };

    let mut decision = CompileDecision::Admit;
    for &child in kind.children() {
        decision = meet(
            decision,
            node_decision(
                grammar,
                plan,
                profile,
                predicates,
                relevant_kinds,
                child,
                cache,
            ),
        );
    }
    for predicate in predicates {
        if predicate
            .discharges()
            .iter()
            .any(|k| relevant_kinds.contains(k))
        {
            decision = meet(
                decision,
                verdict_to_decision(predicate.evaluate(grammar, profile, kind)),
            );
        }
    }

    cache.insert(node_id, decision.clone());
    decision
}

/// Composes the capability
/// envelope bottom-up over `plan` (the reified compilation plan `crate::enumerate::
/// enumerate_default` builds) and returns the overall `CompileDecision` — connecting
/// `characterize` (the profile) and `enumerate_default` (the plan) through
/// `registry`.
///
/// # Algorithm
/// 1. `characterize` projects `g` into a `CharacteristicsProfile`.
/// 2. Every observed `CharacteristicKind` whose disposition is NOT `Disposition::Proven` is
///    collected into a `relevant_kinds` set (`node_decision`'s own doc explains why).
/// 3. `plan`'s root is walked bottom-up via `node_decision`: the meet of every node's children's
///    decisions and its own applicable registered predicates.
/// 4. Separately, every OBSERVED non-`Proven` kind that NO registered predicate discharges at all
///    (so step 3 never had an `evaluate` call to make for it — e.g. [`CharacteristicKind::
///    CoOccurrenceConstraint`], which `default_registry` intentionally leaves undischarged since
///    `ConfirmOnly` is already its own resting disposition) is folded in via `disposition_floor`,
///    so a grammar-wide characteristic with no registered predicate at all still pulls the overall
///    decision down.
/// 5. The two folds `meet` into the final, overall `CompileDecision`.
///
/// # Judgment call: constructs with no distinct plan node
/// Several `ConfigPredicate` characteristics (`CharacteristicKind::Compounding`,
/// `CharacteristicKind::UnorderedMorphRuleApplication`, `CharacteristicKind::MprGroupAppend`,
/// `CharacteristicKind::MprGroupOverwrite`) have NO corresponding `crate::plan::PlanNodeKind`
/// in today's `enumerate_default` shape at all — that module's own doc: it only ever mints leaves
/// for the lexicon (per gate group), one per rewrite rule, and the two composite-emission markers,
/// nothing addressed by `MRuleId`/`StratumId`/an mpr-group index. All four now have real predicates
/// (`CompoundingRecursionSafePredicate`, `UnorderedOrderingUnionPredicate`,
/// `MprGroupAppendNonNarrowingPredicate`, `MprGroupOverwritePredicate`) that each scan
/// `CharacteristicsProfile` directly rather than unconditionally refusing. Which specific node
/// the predicate is evaluated against is behaviorally irrelevant here (every one of these
/// predicates ignores `plan_node` and reaches the SAME verdict regardless), and
/// `node_decision`'s per-node walk (which calls every relevant-kind predicate at EVERY node)
/// already folds the result in correctly without needing a `ModelLocation -> NodeId` lookup table
/// for these kinds at all. This is this step's "representative node" case: no lookup was built
/// because none would change the outcome, not because one was skipped for convenience — documented
/// here rather than silently.
///
/// `CharacteristicKind::Epenthesis` is a related but DISTINCT case, corrected here: its own
/// `ModelLocation` (a `PhonRuleDef::Rewrite` rule) DOES get an ordinary
/// `Leaf { fragment: FragmentSpec::RewriteRule { rule }, .. }` (minted unconditionally for every
/// `PRuleId` in `prules_in_order`, regardless of LHS shape — no special-casing excludes an
/// empty-LHS rule from `rule_children` below). `EpenthesisStructuralRoutePredicate` still ignores
/// `plan_node` and scans observations directly (same mechanics as the four above), but for a
/// different reason: its own subject matter is not "is THIS rule's own leaf faithfully compiled"
/// (the question `RightToLeftRewriteFaithfulReversalPredicate`/`MetathesisFaithfulSwapPredicate`
/// ask at that exact leaf) — it is the GRAMMAR-WIDE side effect the rule's mere presence has on
/// OTHER rules' own propose route (`crate::emit::probe_would_refuse`/[`crate::emit::
/// structural_candidate_rules`], that predicate's own doc), which no single leaf address captures
/// even though one exists.
/// `CharacteristicKind::CircumfixOutputAction` and `CharacteristicKind::Reduplication` are the
/// SAME "no distinct plan node" shape (peeling and structural-composite resynthesis both happen
/// entirely OUTSIDE the compiled FST, so there is genuinely no plan node to address either by):
/// `CircumfixStructuralCompositePredicate`
/// and `ReduplicationPeelSupportedPredicate`
/// ALSO ignore `plan_node` (same reasoning), but each
/// own `evaluate` reads real per-allomorph structural facts rather than unconditionally refusing —
/// see either predicate's own "Node applicability" doc. `CharacteristicKind::SimultaneousRewrite`
/// is the one kind that DOES need (and gets, via the plan walk itself) a SPECIFIC node — see
/// `node_decision`'s own doc for how that mapping actually happens.
///
/// # Deriving the profile
/// This entry point derives a fresh `crate::grammar_semantics::GrammarSemantics` (and therefore a
/// fresh `characterize` walk) for `g` on every call. A caller that evaluates SEVERAL plans against
/// ONE grammar -- `crate::selection::select_plan` is exactly that -- must call
/// `compose_envelope_with_semantics` with a semantics it derived once, or it pays for the whole
/// grammar walk, real `Simultaneous` FST construction included, per candidate.
pub fn compose_envelope(g: &Grammar, plan: &Plan, registry: &PredicateRegistry) -> CompileDecision {
    compose_envelope_with_semantics(&GrammarSemantics::derive(g), plan, registry)
}

/// `compose_envelope` over an already-derived `GrammarSemantics` -- the primary form, so a
/// caller with several plans for one grammar characterizes ONCE. Behaviorally identical:
/// `compose_envelope` is this function with a freshly derived owner.
///
/// # This is a DERIVED fact
/// There is no such thing as a compiler-independent capability verdict: `Disposition::ConfirmOnly`
/// is defined as *"recall-preserving only if the proposer proposes the superset"*, which is a claim
/// about a proposer. So the primary judgement is per-`crate::enumerate::EmissionStrategy`
/// (`compose_envelope_for_strategy`), and this whole-grammar answer is derived from those:
/// `StrategyEnvelope::global`, i.e. the BEST any compiler can offer, refusing only when every
/// compiler refuses. `StrategyEnvelope::declining` says which compiler declined and why, which a
/// scalar decision cannot. That this derivation returns what the compiler-blind form returned is
/// pinned by `per_strategy_derivation_is_identical_on_every_conformance_fixture`.
pub fn compose_envelope_with_semantics(
    semantics: &GrammarSemantics<'_>,
    plan: &Plan,
    registry: &PredicateRegistry,
) -> CompileDecision {
    let grammar_wide = baseline_grammar_wide_checks();
    compose_envelope_across_strategies(
        semantics,
        plan,
        &CapabilityContributions::new(registry, &grammar_wide),
    )
    .global()
}

// Narrowing a predicate away lands its kind on the default-disposition floor, never out of account.
fn compose_over_predicates(
    semantics: &GrammarSemantics<'_>,
    plan: &Plan,
    predicates: &[&dyn CapabilityPredicate],
) -> CompileDecision {
    let profile = semantics.characteristics();
    let relevant_kinds: HashSet<CharacteristicKind> = profile
        .observations()
        .iter()
        .filter(|o| o.disposition != Disposition::Proven)
        .map(|o| o.kind)
        .collect();

    let mut cache = HashMap::new();
    let mut decision = match plan.root() {
        Some(root) => node_decision(
            semantics.grammar(),
            plan,
            profile,
            predicates,
            &relevant_kinds,
            root,
            &mut cache,
        ),
        None => CompileDecision::Admit,
    };

    for &kind in &relevant_kinds {
        if !predicates.iter().any(|p| p.discharges().contains(&kind)) {
            decision = meet(decision, disposition_floor(kind.default_disposition()));
        }
    }

    decision
}

// Positions, not references, so two strategies' predicate sets compare cheaply for walk reuse.
fn constraining_predicate_indices(
    registry: &PredicateRegistry,
    strategy: EmissionStrategy,
) -> Vec<usize> {
    registry
        .predicates()
        .iter()
        .enumerate()
        .filter(|(_, p)| p.constrains_strategies().contains(&strategy))
        .map(|(i, _)| i)
        .collect()
}

// `Proven` kinds are folded in too: a compiler that cannot emit an affix has not earned `Proven`.
fn with_strategy_coverage(
    semantics: &GrammarSemantics<'_>,
    strategy: EmissionStrategy,
    mut decision: CompileDecision,
) -> CompileDecision {
    let observed: HashSet<CharacteristicKind> = semantics
        .characteristics()
        .observations()
        .iter()
        .map(|o| o.kind)
        .collect();

    // `CharacteristicKind::ALL` order, not the set's: a `Refuse`'s diagnostic `Vec` order is observable.
    for &kind in CharacteristicKind::ALL {
        if !observed.contains(&kind) {
            continue;
        }
        decision = meet(decision, strategy_floor(strategy, kind));
    }

    decision
}

/// Folds every `checks` entry scoped to `strategy` into `decision` via `meet`.
fn fold_grammar_wide_checks(
    mut decision: CompileDecision,
    semantics: &GrammarSemantics<'_>,
    plan: &Plan,
    strategy: EmissionStrategy,
    checks: &[Box<dyn GrammarWideCheck>],
) -> CompileDecision {
    for check in checks.iter().filter(|c| c.strategy() == strategy) {
        if let Some(refusal) = check.evaluate(semantics, plan) {
            decision = meet(decision, refusal);
        }
    }
    decision
}

const TEMPLATED_UNSUPPORTED_SHAPE_PREDICATE: &str = "strategy-coverage.templated-unsupported-shape";

/// Applies shape-specific eligibility checks to templated emission.
fn templated_shape_floor(semantics: &GrammarSemantics<'_>) -> CompileDecision {
    let grammar = semantics.grammar();
    let mut diagnostics = Vec::new();
    let mut role_floor_refusals = HashSet::new();
    for (rule_index, rule) in grammar.mrules.iter().enumerate() {
        let Some(allomorphs) = rule.affix_allomorphs() else {
            continue;
        };
        for (allomorph_index, allomorph) in allomorphs.iter().enumerate() {
            let role = crate::emit::classify_affix(&allomorph.rhs);
            let reason = match role {
                crate::emit::Role::Infix => Some(
                    "Role::Infix is handled only by the emitter's uncovered-role branch; the \
                     templated proposer has no Copy-Insert-Copy/infix entry",
                ),
                crate::emit::Role::CircumfixPrefix => {
                    unsupported_templated_circumfix_reason(allomorph)
                }
                _ => None,
            };
            let Some(reason) = reason else {
                continue;
            };
            role_floor_refusals.insert((rule_index, allomorph_index));
            diagnostics.push(CapabilityDiagnostic {
                predicate: TEMPLATED_UNSUPPORTED_SHAPE_PREDICATE,
                construct: format!(
                    "mrule {} allomorph #{} ({role:?})",
                    rule_index, allomorph_index
                ),
                witness: format!(
                    "no faithful templated emission path: {reason}; \
                     emit_underlying_templated skips structural composite lowering, so this \
                     allomorph emits no faithful candidate"
                ),
            });
        }
    }

    let active_table = grammar.strata.last().map(|stratum| stratum.table);
    match active_table {
        None => diagnostics.push(CapabilityDiagnostic {
            predicate: TEMPLATED_UNSUPPORTED_SHAPE_PREDICATE,
            construct: "grammar (missing final active pipeline table)".to_string(),
            witness:
                "no faithful templated emission path: grammar has no final active pipeline table"
                    .to_string(),
        }),
        Some(active_table) if grammar.char_tables.get(active_table.0 as usize).is_none() => {
            diagnostics.push(CapabilityDiagnostic {
                predicate: TEMPLATED_UNSUPPORTED_SHAPE_PREDICATE,
                construct: format!("grammar (invalid final active pipeline table {active_table:?})"),
                witness: format!(
                    "no faithful templated emission path: final active pipeline table {active_table:?} is not defined"
                ),
            });
        }
        Some(active_table) => {
            for (rule_index, rule) in grammar.mrules.iter().enumerate() {
                let Some(allomorphs) = rule.affix_allomorphs() else {
                    continue;
                };
                for (allomorph_index, allomorph) in allomorphs.iter().enumerate() {
                    if role_floor_refusals.contains(&(rule_index, allomorph_index)) {
                        continue;
                    }
                    // `emit_rule_allomorphs`'s ordinary literal-insert branch already handles a plain Prefix/Suffix/None role regardless of this classifier's verdict; consulting it here would over-claim a refusal the real emission path never makes (see `pattern_root_token_route_gate.rs`'s `guesser-pattern-root-fallback` case).
                    if matches!(
                        crate::emit::classify_affix(&allomorph.rhs),
                        crate::emit::Role::Prefix
                            | crate::emit::Role::Suffix
                            | crate::emit::Role::None
                    ) {
                        continue;
                    }
                    let crate::structural_allomorph::MorphologyRewrite::Unsupported {
                        shape_id,
                        reason_id,
                        ..
                    } = crate::structural_allomorph::MorphologyRewriteClassifier::classify(
                        grammar,
                        allomorph,
                        active_table,
                    )
                    else {
                        continue;
                    };
                    diagnostics.push(CapabilityDiagnostic {
                        predicate: TEMPLATED_UNSUPPORTED_SHAPE_PREDICATE,
                        construct: format!(
                            "mrule {} allomorph #{} ({} morphology relation)",
                            rule_index, allomorph_index, shape_id
                        ),
                        witness: format!(
                            "no faithful templated emission path: morphology relation classifier \
                             rejected {shape_id}/{reason_id} against final active pipeline table \
                             {active_table:?}"
                        ),
                    });
                }
            }

            if let Err(error) =
                crate::structural_allomorph::MorphologyRelationPlan::build(grammar, active_table)
            {
                if !matches!(
                    error,
                    crate::structural_allomorph::MorphologyRelationError::UnsupportedRewrite { .. }
                ) {
                    diagnostics.push(CapabilityDiagnostic {
                        predicate: TEMPLATED_UNSUPPORTED_SHAPE_PREDICATE,
                        construct: "morphology relation plan".to_string(),
                        witness: format!(
                            "no faithful templated emission path: morphology relation plan \
                             cannot be constructed against final active pipeline table \
                             {active_table:?}: {error}"
                        ),
                    });
                }
            }
        }
    }

    // Owned upstream by `UnorderedOrderingUnionPredicate`; re-deriving it here duplicated and contradicted that owner (`docs/research/conformance-containment-inventory.md`).

    for (prule_index, prule) in grammar.prules.iter().enumerate() {
        let PhonRuleDef::Rewrite(rule) = prule else {
            continue;
        };
        if !rule.lhs.nodes.is_empty() {
            continue;
        }
        for (subrule_index, subrule) in rule.subrules.iter().enumerate() {
            if !subrule.self_opaquing {
                continue;
            }
            diagnostics.push(CapabilityDiagnostic {
                predicate: TEMPLATED_UNSUPPORTED_SHAPE_PREDICATE,
                construct: format!("prule {prule_index} subrule {subrule_index} (Epenthesis)"),
                witness: "no faithful templated emission path: self-opaquing epenthesis needs \
                          analysis-side fixpoint reapplication, but templated compilation skips \
                          that fixpoint route; this occurrence emits no faithful candidate"
                    .to_string(),
            });
        }
    }
    if diagnostics.is_empty() {
        CompileDecision::Admit
    } else {
        CompileDecision::Refuse(diagnostics)
    }
}

fn unsupported_templated_circumfix_reason(allomorph: &AffixAllomorphDef) -> Option<&'static str> {
    if allomorph.lhs.len() != 1 {
        return Some("the circumfix has a multi-part LHS, which requires root-internal splitting");
    }

    let copies: Vec<(usize, &PartRef)> = allomorph
        .rhs
        .iter()
        .enumerate()
        .filter_map(|(position, action)| match action {
            OutputAction::Copy(part) => Some((position, part)),
            _ => None,
        })
        .collect();
    if copies.len() != 1 {
        return Some("the circumfix repeats or omits its root copy, which requires duplication");
    }

    let (copy_position, part) = copies[0];
    if !matches!(part, PartRef::Input(0)) {
        return Some("the circumfix copy is not the whole root input");
    }
    if copy_position == 0 || copy_position + 1 == allomorph.rhs.len() {
        return Some("the circumfix does not wrap a root copy on both sides");
    }
    if allomorph.rhs[..copy_position]
        .iter()
        .chain(allomorph.rhs[copy_position + 1..].iter())
        .any(|action| !matches!(action, OutputAction::InsertSegments { .. }))
    {
        return Some("the circumfix has a non-insert action around its root copy");
    }

    None
}

/// One compiler's verdict for one grammar+plan: the primary unit of capability judgement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyVerdict {
    pub strategy: EmissionStrategy,
    pub decision: CompileDecision,
}

/// Every `crate::enumerate::EmissionStrategy`'s own verdict for one grammar+plan, and the
/// whole-grammar verdict derived from them.
///
/// This is the shape the capability gate actually has, made explicit. A single scalar decision
/// cannot express "this grammar is compilable, but not by the compiler you were about to use" nor
/// "no compiler can do this, and here is what each one choked on" — both of which are ordinary
/// states for a three-compiler crate, and the second of which is what a caller needs to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyEnvelope {
    verdicts: Vec<StrategyVerdict>,
}

impl StrategyEnvelope {
    /// Every strategy's verdict, in `crate::strategy_coverage::ALL_STRATEGIES` order.
    pub fn verdicts(&self) -> &[StrategyVerdict] {
        &self.verdicts
    }

    /// `strategy`'s own verdict, or `None` if it was not composed — which cannot happen for an
    /// `ALL_STRATEGIES` member of an envelope from `compose_envelope_across_strategies`;
    /// pinned by `per_strategy_derivation_is_identical_on_every_conformance_fixture`.
    pub fn decision_for(&self, strategy: EmissionStrategy) -> Option<&CompileDecision> {
        self.verdicts
            .iter()
            .find(|v| v.strategy == strategy)
            .map(|v| &v.decision)
    }

    /// Every strategy that refused, with its diagnostics — the "which compiler declined, for what
    /// reason" report `global`'s scalar answer cannot carry.
    pub fn declining(&self) -> Vec<(EmissionStrategy, &[CapabilityDiagnostic])> {
        self.verdicts
            .iter()
            .filter_map(|v| match &v.decision {
                CompileDecision::Refuse(diagnostics) => Some((v.strategy, diagnostics.as_slice())),
                CompileDecision::Admit | CompileDecision::ConfirmOnly => None,
            })
            .collect()
    }

    /// The whole-grammar verdict: the BEST any compiler offers, so `Refuse` iff EVERY strategy
    /// refuses — pinned by `global_refuses_only_when_every_strategy_refuses`. The dual of `meet` —
    /// a join, because a grammar one compiler can handle is compilable even if another cannot.
    ///
    /// A refusing result carries the diagnostics EVERY refusing strategy shares — the reasons that
    /// hold no matter which compiler is chosen, which is exactly the claim a whole-grammar refusal
    /// makes. When the strategies refuse for entirely disjoint reasons that intersection is empty,
    /// and an empty-diagnostic refusal would be unactionable, so the union is reported instead;
    /// `declining` keeps the per-strategy attribution either way.
    pub fn global(&self) -> CompileDecision {
        let mut refusals: Vec<&[CapabilityDiagnostic]> = Vec::new();
        let mut confirm_only = false;
        for verdict in &self.verdicts {
            match &verdict.decision {
                CompileDecision::Admit => return CompileDecision::Admit,
                CompileDecision::ConfirmOnly => confirm_only = true,
                CompileDecision::Refuse(diagnostics) => refusals.push(diagnostics),
            }
        }
        if confirm_only {
            return CompileDecision::ConfirmOnly;
        }
        // No strategies at all is vacuous, not a refusal -- there is no compiler to have declined.
        let Some((first, rest)) = refusals.split_first() else {
            return CompileDecision::Admit;
        };
        let shared: Vec<CapabilityDiagnostic> = first
            .iter()
            .filter(|&d| rest.iter().all(|other| other.contains(d)))
            .cloned()
            .collect();
        if !shared.is_empty() {
            return CompileDecision::Refuse(shared);
        }
        let mut union: Vec<CapabilityDiagnostic> = Vec::new();
        for diagnostics in &refusals {
            for d in *diagnostics {
                if !union.contains(d) {
                    union.push(d.clone());
                }
            }
        }
        CompileDecision::Refuse(union)
    }
}

/// THE primary capability judgement: every compiler's own verdict for `plan`, each computed
/// DIRECTLY from the predicates that constrain that compiler met with that compiler's
/// `crate::strategy_coverage` rows.
///
/// # Why this is not a global verdict, narrowed
/// It used to be. `compose_envelope_for_strategy` computed a strategy-BLIND decision first and
/// `meet`-narrowed the per-strategy rows into it, which meant a per-compiler answer could only ever
/// be worse than the blind one — a predicate that describes a shape only a PROTOTYPE compiler can
/// exhibit still refused the mainline compiler, and no amount of per-strategy evidence could undo
/// it. Composing each strategy from its own predicate set removes that floor: the blind verdict is
/// not an input to any per-strategy answer any more, it is `StrategyEnvelope::global`'s output.
///
/// # Cost
/// The plan walk is run once per DISTINCT predicate set, not once per strategy. With every
/// predicate at `CapabilityPredicate::constrains_strategies`'s default (all strategies) there is
/// exactly one such set, so this costs the same single walk the strategy-blind form did — the
/// per-candidate cost `crate::selection` and `crate::grammar_semantics` care about does not move
/// until a predicate is genuinely reclassified.
pub fn compose_envelope_across_strategies(
    semantics: &GrammarSemantics<'_>,
    plan: &Plan,
    contributions: &CapabilityContributions<'_>,
) -> StrategyEnvelope {
    let mut walked: Vec<(Vec<usize>, CompileDecision)> = Vec::new();
    let mut verdicts = Vec::with_capacity(ALL_STRATEGIES.len());

    for &strategy in ALL_STRATEGIES {
        let indices = constraining_predicate_indices(contributions.predicates, strategy);
        let base = match walked.iter().find(|(seen, _)| *seen == indices) {
            Some((_, decision)) => decision.clone(),
            None => {
                let predicates: Vec<&dyn CapabilityPredicate> = indices
                    .iter()
                    .map(|&i| contributions.predicates.predicates()[i].as_ref())
                    .collect();
                let decision = compose_over_predicates(semantics, plan, &predicates);
                walked.push((indices, decision.clone()));
                decision
            }
        };
        let decision = with_strategy_coverage(semantics, strategy, base);
        let decision = fold_grammar_wide_checks(
            decision,
            semantics,
            plan,
            strategy,
            contributions.grammar_wide,
        );
        verdicts.push(StrategyVerdict { strategy, decision });
    }

    StrategyEnvelope { verdicts }
}

/// ONE compiler's capability verdict for `plan`: the predicates that constrain `strategy`, met with
/// `strategy`'s own rows in the per-strategy construct account `crate::strategy_coverage` owns.
///
/// # Why the judgement is per-compiler
/// `Disposition::ConfirmOnly`'s own definition is *"Recall-preserving only if the proposer
/// proposes the superset."* That precondition is a claim about a PROPOSER, so there is no
/// compiler-independent verdict to be had: a disposition checked without a strategy in hand is
/// being checked against the UNION of every compiler's abilities. `Compounding` rested at a
/// non-refusing disposition on the strength of `crate::emit`'s compilers while `crate::uflexc` --
/// the only lexicon emitter `crate::enumerate::EmissionStrategy::PlanComposed` has -- could not
/// propose a compound at all. The hole survived because nothing could express the question.
///
/// # Composed, not narrowed
/// This function does NOT compute a whole-grammar decision and then restrict it. The predicate set
/// is filtered by `CapabilityPredicate::constrains_strategies` FIRST and the plan is walked with
/// that set, so a predicate that constrains only some compilers cannot refuse the others. The
/// whole-grammar answer runs the other way: `compose_envelope_with_semantics` is
/// `StrategyEnvelope::global` over these verdicts.
///
/// # What the coverage account contributes
/// Every OBSERVED `CharacteristicKind` is looked up in
/// `crate::strategy_coverage::representation_of` and folded in via `meet`:
/// `crate::strategy_coverage::StrategyRepresentation::Represents` contributes `Admit`,
/// `RepresentsWithKnownGap` contributes `CompileDecision::ConfirmOnly`, and `CannotRepresent`
/// contributes a `CompileDecision::Refuse` naming the strategy, the construct and the citation.
///
/// # Memoization
/// Takes the SAME `GrammarSemantics` every strategy shares. The grammar-only
/// `GrammarSemantics::characteristics` memo is deliberately NOT re-keyed on the strategy: see
/// `crate::strategy_coverage`'s module doc for the full argument (in short -- `characterize`
/// answers "which constructs does the grammar contain", which cannot vary by compiler, while the
/// strategy-dependent half has no grammar input at all, so the two are split rather than merged).
pub fn compose_envelope_for_strategy(
    semantics: &GrammarSemantics<'_>,
    plan: &Plan,
    strategy: EmissionStrategy,
    registry: &PredicateRegistry,
) -> CompileDecision {
    let predicates: Vec<&dyn CapabilityPredicate> =
        constraining_predicate_indices(registry, strategy)
            .into_iter()
            .map(|i| registry.predicates()[i].as_ref())
            .collect();
    let base = compose_over_predicates(semantics, plan, &predicates);
    let decision = with_strategy_coverage(semantics, strategy, base);
    fold_grammar_wide_checks(
        decision,
        semantics,
        plan,
        strategy,
        &baseline_grammar_wide_checks(),
    )
}

/// `strategy_floor`'s own refusal id -- not a registered `CapabilityPredicate` or `GrammarWideCheck` (it has no plan node and applies identically to every strategy), so `crate::backend_selection::capability_shape_key` reads this constant directly rather than the registry lookup those two shapes go through.
pub(crate) const STRATEGY_FLOOR_NOT_REPRESENTABLE_PREDICATE: &str =
    "strategy-coverage.construct-not-representable";
/// The shape key `crate::backend_selection::capability_shape_key` returns for [`STRATEGY_FLOOR_NOT_REPRESENTABLE_PREDICATE`].
pub(crate) const STRATEGY_FLOOR_NOT_REPRESENTABLE_SHAPE_KEY: &str = "nonregular-process-morphology";

// The per-strategy account's contribution alone; `compose_over_predicates` folds in the dispositions.
fn strategy_floor(strategy: EmissionStrategy, kind: CharacteristicKind) -> CompileDecision {
    use crate::strategy_coverage::StrategyRepresentation;

    let row = crate::strategy_coverage::representation_of(strategy, kind);
    match row.representation {
        StrategyRepresentation::Represents => CompileDecision::Admit,
        StrategyRepresentation::RepresentsWithKnownGap => CompileDecision::ConfirmOnly,
        StrategyRepresentation::CannotRepresent => {
            CompileDecision::Refuse(vec![CapabilityDiagnostic {
                predicate: STRATEGY_FLOOR_NOT_REPRESENTABLE_PREDICATE,
                construct: format!("{kind:?}"),
                witness: format!(
                    "EmissionStrategy::{strategy:?} ({}) cannot represent {kind:?}: its proposer \
                     emits nothing for this construct, so the ConfirmOnly precondition (\"the \
                     proposer proposes the superset\") is FALSE for this compiler and confirm has \
                     no candidate to prune. Evidence: {}. Another strategy may well represent it \
                     -- that is exactly the inheritance this account exists to stop.",
                    strategy.label(),
                    row.evidence
                ),
            }])
        }
    }
}

#[cfg(test)]
mod tests;
