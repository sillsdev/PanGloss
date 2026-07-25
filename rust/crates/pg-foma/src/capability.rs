//! Step 1 of `openspec/changes/add-capability-characteristics-check` (the keystone gate, ADR
//! 0001): the [`CharacteristicsProfile`] projection, the [`CapabilityPredicate`] trait +
//! [`PredicateVerdict`], the exhaustive default-deny [`characterize`], and the worked
//! `simultaneous.subrule-overlap` predicate (design.md D3).
//!
//! **Purely additive.** This module defines and unit-tests the types and characterizer only — it
//! does NOT wire a gate into any production compile path (`emit.rs`/`gate.rs`/`replace.rs`/
//! `preexpand.rs` bodies are untouched), the way `crate::plan` (Step 1 of
//! `reify-compilation-plans`) defines its `Plan` data type without rewiring anything either. A
//! later step composes the envelope bottom-up over `crate::plan::Plan` (design.md D4) and flips a
//! real compile seam to consult it — see that change's `tasks.md`.
//!
//! # D1: the characteristics projection
//! [`characterize`] walks a [`Grammar`] and matches **every** variant of **every** frozen
//! `model.rs` enum design.md D1 names, with **no catch-all arm** — the discipline that would have
//! caught the `Compounding` silent-recall hole (design.md's own words). Adding a new `model.rs`
//! variant to any of those enums breaks THIS file's build until [`characterize`] (or one of its
//! private per-construct helpers) is updated to give it an explicit [`Disposition`] — see this
//! module's tests for a from-scratch check of that property against
//! [`pg_grammar::model::ReduplicationHint`]/[`pg_grammar::model::OutputAction`]/etc.
//!
//! # D2: the predicate trait + verdict
//! [`CapabilityPredicate`] is D2's oracle-verified proof-obligation trait: conservative by
//! construction (`evaluate` may return [`PredicateVerdict::Refuse`] too eagerly, never
//! [`PredicateVerdict::Admit`] too eagerly). [`PredicateRegistry`]/[`undischarged_kinds`] give the
//! "no silent vacuous pass" coverage check design.md D2 requires: every `FailClosed`/
//! `ConfigPredicate` [`CharacteristicKind`] must be named by at least one registered predicate's
//! [`CapabilityPredicate::discharges`].
//!
//! # D3: the worked example
//! [`SimultaneousSubruleOverlapPredicate`] implements the `simultaneous.subrule-overlap` predicate
//! design.md D3 works through in full, now via the REAL automaton intersection
//! `lower-fst-pattern-environments` (Stage 1B, [`crate::lower`]) provides — see that type's own
//! doc for how the intersection runs and [`LoweredSpan`]'s doc for where the lowering itself
//! happens (`characterize`, not `evaluate`).
//!
//! # `PlanNode` vs. `PlanNodeKind`
//! design.md D2's pseudocode signature is `evaluate(&self, profile: &CharacteristicsProfile,
//! plan_node: &PlanNode) -> PredicateVerdict`. `crate::plan` has no type literally named
//! `PlanNode` (its closed node-kind enum is [`crate::plan::PlanNodeKind`]; a node's *identity* is
//! its separately-interned [`crate::plan::NodeId`]) — this module's trait takes
//! `&PlanNodeKind` where D2 says `&PlanNode`, which is the concrete type D2's own co-designed
//! `crate::plan` module actually shipped. Flagged as a judgment call for review, not silently
//! reconciled.
//!
//! # D4 (Step 2): bottom-up envelope composition + the CHECK-ONLY [`CompileDecision`]
//! [`compose_envelope`] is this crate's Step 2 of `add-capability-characteristics-check`: it runs
//! [`characterize`] to get the profile, walks `crate::enumerate::enumerate_default`'s reified
//! [`crate::plan::Plan`] bottom-up (design.md D4: "a node's verdict is the meet of its children's
//! verdicts and its own node-level predicate"), and folds in every observed non-`Proven`
//! characteristic that has no plan-node-addressable predicate at all. [`meet`] is D4's lattice
//! made explicit (`Refuse` dominates `ConfirmOnly` dominates `Admit`); [`CompileDecision`] widens
//! [`PredicateVerdict`]'s single-diagnostic `Refuse` into a deduplicated `Vec` so a caller sees
//! every refusing construct in one pass. **Still purely additive and check-only**: nothing in this
//! crate consults [`CompileDecision`] to block or alter any production compile path — the
//! production flip, ADR 0005's override, and the CI cross-check are later `tasks.md` items. D4's
//! third paragraph (interaction predicates for `Union`/`Compose` nodes, via parallel-independence)
//! is deliberately NOT implemented here: no interaction predicate exists in [`default_registry`]
//! yet — `lower-fst-pattern-environments` (Stage 1B, [`crate::lower`]) now exists and
//! [`SimultaneousSubruleOverlapPredicate`] uses it, but no `Union`/`Compose`-level interaction
//! predicate has been built ON TOP of that seam yet — so a `Union`/`Compose` node's "own predicate
//! verdicts" are simply empty today — flagged as a documented gap, not a silent omission; see
//! [`compose_envelope`]'s own doc for the per-construct plan-node-mapping judgment calls.

use std::collections::{HashMap, HashSet};

use pg_grammar::model::{
    AffixAllomorphDef, AllomorphId, Dir, Grammar, MRuleId, MorphRuleDef, MorphRuleOrder,
    MprGroupMatchType, MprGroupOutput, MprSet, NatClassId, NaturalClassKind, OutputAction,
    PRuleId, PartRef, PhonRuleDef, ReduplicationHint, RewriteMode, StratumId,
};

use crate::plan::{FragmentSpec, NodeId, Plan, PlanNodeKind};

// =================================================================================================
// D1: Disposition + CharacteristicKind + the characterizer
// =================================================================================================

/// A characteristic's capability disposition (design.md D1). Ordered here from "most trusted" to
/// "least" purely for reading convenience — no code relies on `Disposition`'s ordinal value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Disposition {
    /// Proven faithful; no predicate needed, admission-filtering allowed unconditionally.
    Proven,
    /// Compiles conditionally: `ConfirmOnly` unless/until a registered predicate proves `Admit`
    /// for the specific configuration observed (design.md D2's landing-spot verdict).
    ConfigPredicate,
    /// Recall-preserving only if the proposer proposes the superset (no proven no-false-negative
    /// admission filter) — a first-class, non-failure verdict (ADR 0001).
    ConfirmOnly,
    /// Hard compile-time refusal by default; only ADR 0005's explicit, indelibly-stamped override
    /// force-compiles it.
    FailClosed,
}

/// The closed set of observed grammar characteristics (design.md D1's table, one variant per
/// table row/enum-family). Deliberately **not** one variant per individual `model.rs` enum
/// *variant* in every case — where D1's own table collapses several variants of one enum into a
/// single named characteristic (e.g. `OutputAction`'s four variants all feed "output-action
/// kind"), this enum mirrors that collapse; [`characterize`]'s per-variant `match` arms still stay
/// individually written (no catch-all), so the exhaustiveness discipline holds at the `model.rs`
/// level even where several arms produce the same [`CharacteristicKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharacteristicKind {
    /// `MorphRuleDef::AffixProcess` (model.rs:543).
    Affixation,
    /// `MorphRuleDef::Realizational` (model.rs:546).
    RealizationalMorphology,
    /// `MorphRuleDef::Compounding` (model.rs:544). D5's first act: FailClosed.
    Compounding,
    /// `MorphRuleOrder::Linear` (model.rs:1058).
    OrderedMorphRuleApplication,
    /// `MorphRuleOrder::Unordered` (model.rs:1059). D5's first act: FailClosed.
    UnorderedMorphRuleApplication,
    /// `MprGroupOutput::Append` (model.rs:833).
    MprGroupAppend,
    /// `MprGroupOutput::Overwrite` (model.rs:832). D5's first act ("MprGroup...FailClosed").
    MprGroupOverwrite,
    /// `RewriteMode::Iterative` (model.rs:386).
    IterativeRewrite,
    /// `RewriteMode::Simultaneous` (model.rs:387). Discharged by
    /// [`SimultaneousSubruleOverlapPredicate`] (design.md D3).
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
    /// discontinuous material, D1's "cover-circumfix-null-..." row. NOT raised for every
    /// `OutputAction` occurrence (see [`allomorph_drops_lhs_material`]'s doc for why that would be
    /// unsound-by-over-triggering).
    CircumfixOutputAction,
    /// An `AffixAllomorphDef` whose RHS truly reduplicates: some `Input` part is echoed by
    /// `Copy`/`Modify` actions >= 2 times (model.rs:679's `ReduplicationHint`). NOT raised for
    /// every allomorph carrying a `ReduplicationHint` value (see [`rhs_has_true_reduplication`]'s
    /// doc — `Implicit` is the DTD default for every non-reduplicating affix too).
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
    /// [`MultiTableFaithfulThreadingPredicate`] (`openspec/changes/fix-multitable-fst-compilation`).
    /// See that predicate's own doc for the admit/confirm-only/refuse split.
    MultiTable,
}

impl CharacteristicKind {
    /// Every [`CharacteristicKind`] variant — hand-maintained (Rust has no enum reflection), so
    /// adding a variant above and forgetting to add it here is a real gap [`undischarged_kinds`]
    /// cannot see. [`crate::capability::tests::all_kinds_have_a_default_disposition`] is the
    /// closest available backstop (it re-derives disposition via [`Self::default_disposition`],
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
    ];

    /// design.md D1's table, as code: this characteristic's disposition BEFORE any predicate runs.
    /// Exhaustively matched (no catch-all) — adding a `CharacteristicKind` variant breaks this
    /// build too, same discipline as [`characterize`]'s own `model.rs` matches.
    pub fn default_disposition(self) -> Disposition {
        match self {
            CharacteristicKind::Affixation => Disposition::Proven,
            CharacteristicKind::RealizationalMorphology => Disposition::ConfirmOnly,
            CharacteristicKind::Compounding => Disposition::FailClosed,
            CharacteristicKind::OrderedMorphRuleApplication => Disposition::Proven,
            CharacteristicKind::UnorderedMorphRuleApplication => Disposition::FailClosed,
            CharacteristicKind::MprGroupAppend => Disposition::ConfirmOnly,
            CharacteristicKind::MprGroupOverwrite => Disposition::FailClosed,
            CharacteristicKind::IterativeRewrite => Disposition::Proven,
            CharacteristicKind::SimultaneousRewrite => Disposition::ConfigPredicate,
            CharacteristicKind::LeftToRightRewrite => Disposition::Proven,
            CharacteristicKind::RightToLeftRewrite => Disposition::FailClosed,
            CharacteristicKind::Metathesis => Disposition::FailClosed,
            CharacteristicKind::Epenthesis => Disposition::ConfigPredicate,
            CharacteristicKind::SubruleGating => Disposition::Proven,
            CharacteristicKind::CircumfixOutputAction => Disposition::ConfigPredicate,
            CharacteristicKind::Reduplication => Disposition::FailClosed,
            CharacteristicKind::CoOccurrenceConstraint => Disposition::ConfirmOnly,
            CharacteristicKind::NaturalClassDefinition => Disposition::Proven,
            // `fix-multitable-fst-compilation`: rewrite-rule compilation now threads each rule's
            // own owning table faithfully (no more implicit table-zero default), so multi-table is
            // no longer bare FailClosed -- but no no-false-positive admission-filter proof exists
            // yet (ADR 0001), so the resting disposition is the ConfigPredicate landing spot:
            // ConfirmOnly unless/until `MultiTableFaithfulThreadingPredicate` proves `Admit` for
            // the specific configuration observed (pairwise-disjoint table representations).
            CharacteristicKind::MultiTable => Disposition::ConfigPredicate,
        }
    }
}

/// Which `model.rs` construct occurrence induced a [`CharacteristicObservation`] (design.md D1:
/// "each tagged with the model location(s) that induced it").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelLocation {
    MorphRule(MRuleId),
    /// One allomorph of an `AffixProcess`/`Realizational` rule (`MorphRuleDef::affix_allomorphs`).
    AffixAllomorph { rule: MRuleId, allomorph_index: usize },
    Stratum(StratumId),
    /// Index into `Grammar::mpr_groups`.
    MprGroup(usize),
    PhonRule(PRuleId),
    RewriteSubrule { rule: PRuleId, subrule_index: usize },
    NaturalClass(NatClassId),
    /// Index into `Grammar::morphemes` whose `co_occurrence` list this observation came from.
    MorphemeCoOccurrence(usize),
    AllomorphCoOccurrence(AllomorphId),
}

/// A subrule's D3 `span(s) = left_env · lhs_focus · right_env`, pre-lowered at [`characterize`]
/// time via [`crate::lower::lower_span`] (Stage 1B, `lower-fst-pattern-environments`) into the
/// `(left_language, focus_right_language)` pair [`crate::lower::spans_overlap`] intersects.
///
/// Lowered HERE (inside `characterize`, which walks the `&Grammar` directly) rather than lazily
/// inside [`SimultaneousSubruleOverlapPredicate::evaluate`] itself: [`CapabilityPredicate::
/// evaluate`]'s signature (design.md D2, exactly) takes only `&CharacteristicsProfile`/
/// `&PlanNodeKind` — no `&Grammar`/`SegAlphabet`/`FomaOptions`, everything `lower_span` needs to
/// run. Pre-lowering into the profile (which the trait's OWN doc already calls "a self-contained
/// projection", design.md D1) keeps that generic trait signature untouched rather than widening it
/// crate-wide for one predicate's sake. Flagged as a judgment call for review (the same kind
/// `crate::lower`'s own doc names for its `PlanNode`/`PlanNodeKind` naming gap), not silently
/// reconciled: a cleaner long-term shape might carry `&Grammar`/an alphabet through
/// [`CapabilityPredicate::evaluate`] itself once more predicates need Stage-1B lowering, but that
/// is a wider trait change this additive step does not take.
#[derive(Debug, Clone)]
pub enum LoweredSpan {
    /// Lowered successfully to `(left_language, focus_right_language)` — boxed (clippy
    /// `large_enum_variant`): two owned [`foma::types::Fsm`]s make this variant far larger than
    /// [`Self::Unsupported`]'s `String`, and every [`SubruleGateInfo`] carries one of these per
    /// subrule.
    Ok(Box<(foma::types::Fsm, foma::types::Fsm)>),
    /// [`crate::lower::lower_span`] hit a pattern node kind (or a grammar with no character table
    /// at all — see [`lower_subrule_span`]) it cannot represent; the message names the cause.
    Unsupported(String),
}

/// Per-subrule gate/opacity facts a [`RewriteRuleDef`](pg_grammar::model::RewriteRuleDef)'s
/// [`ObservationDetail::SimultaneousRewrite`] carries — exactly what
/// [`SimultaneousSubruleOverlapPredicate`] (D3) needs, without re-walking the `Grammar` at
/// evaluate-time (the profile is meant to be a self-contained projection, design.md D1).
///
/// No longer `Copy` (dropped from the derive by this step): [`LoweredSpan::Ok`] carries owned
/// [`foma::types::Fsm`] values, which are `Clone` but not `Copy` upstream.
#[derive(Debug, Clone)]
pub struct SubruleGateInfo {
    pub index: usize,
    pub required_mpr: MprSet,
    pub excluded_mpr: MprSet,
    pub self_opaquing: bool,
    /// Stage 1B (`lower-fst-pattern-environments`): this subrule's pre-lowered D3 span.
    pub span: LoweredSpan,
}

/// [`ObservationDetail::SimultaneousRewrite`]'s payload: one rule's full subrule-gate table.
#[derive(Debug, Clone)]
pub struct SimultaneousRewriteDetail {
    pub rule: PRuleId,
    pub subrules: Vec<SubruleGateInfo>,
}

/// [`ObservationDetail::MultiTable`]'s payload
/// (`openspec/changes/fix-multitable-fst-compilation`): the structural fact
/// [`MultiTableFaithfulThreadingPredicate`] needs, computed once here rather than re-derived at
/// `evaluate` time (this profile is meant to be a self-contained projection, design.md D1 — same
/// reasoning [`LoweredSpan`]'s own doc gives for pre-lowering D3's spans).
#[derive(Debug, Clone)]
pub struct MultiTableDetail {
    /// `g.char_tables.len()`.
    pub table_count: usize,
    /// `true` iff NO two distinct tables share a normalized representation (spelling) — the
    /// structural condition [`MultiTableFaithfulThreadingPredicate`]'s own doc explains: per-rule
    /// table-correct resolution (this change's `pg_foma::replace::owning_table` fix) is faithful
    /// with no residual cross-table token-collision risk exactly when every table's own character
    /// inventory is disjoint from every other's.
    pub representations_pairwise_disjoint: bool,
    /// The first shared representation found (any two tables, document order), if
    /// `representations_pairwise_disjoint` is `false` — a concrete witness for the diagnostic,
    /// never just "some tables overlap somewhere".
    pub shared_representation_witness: Option<String>,
}

/// Extra structured data an observation needs beyond `kind`/`disposition`/`location`, for the
/// characteristics that a predicate must inspect at finer grain than "did this occur at all"
/// (design.md D2/D3). Most characteristics carry `None` — [`CharacteristicKind::
/// SimultaneousRewrite`] needs [`Self::SimultaneousRewrite`] (D3's worked example) and
/// [`CharacteristicKind::MultiTable`] needs [`Self::MultiTable`]
/// (`fix-multitable-fst-compilation`).
#[derive(Debug, Clone)]
pub enum ObservationDetail {
    None,
    SimultaneousRewrite(SimultaneousRewriteDetail),
    MultiTable(MultiTableDetail),
}

/// One occurrence of a characteristic in a [`CharacteristicsProfile`] (design.md D1).
#[derive(Debug, Clone)]
pub struct CharacteristicObservation {
    pub kind: CharacteristicKind,
    pub disposition: Disposition,
    pub location: ModelLocation,
    pub detail: ObservationDetail,
}

impl CharacteristicObservation {
    /// `disposition` is always derived from `kind` via [`CharacteristicKind::default_disposition`]
    /// — there is no code path that can push an observation whose disposition disagrees with its
    /// own kind's D1 table entry (a correctness invariant this constructor enforces structurally
    /// rather than by convention at each of [`characterize`]'s many call sites).
    fn new(kind: CharacteristicKind, location: ModelLocation, detail: ObservationDetail) -> Self {
        CharacteristicObservation {
            disposition: kind.default_disposition(),
            kind,
            location,
            detail,
        }
    }
}

/// D1's "cardinality/stem" fields: cheap grammar-scale facts fed to cost/planning, not the
/// correctness gate itself.
#[derive(Debug, Clone, Copy, Default)]
pub struct GrammarCardinality {
    pub entry_count: usize,
    pub morpheme_count: usize,
    pub mrule_count: usize,
    pub prule_count: usize,
    pub stratum_count: usize,
    /// D1: "max reachable derivation chain depth (ADR 0003 dimension — the Aweti 24-level
    /// chain)... if cheaply computable — else leave a documented TODO, do NOT invent an unsound
    /// estimate." Computing this for real needs the morphotactic reachability automaton
    /// (`crate::morphotactics`/`pg_rules::stratum`) — a genuine per-grammar graph analysis, not a
    /// field lookup, and out of scope for this purely-additive data-modeling step. Left `None`
    /// rather than guessed; a later step should wire in the real computation here.
    pub max_derivation_chain_depth: Option<usize>,
}

/// D1's full projection: every observed characteristic plus the grammar's cardinality facts.
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
        self.observations.iter().any(|o| o.disposition == disposition)
    }

    /// Every distinct [`CharacteristicKind`] observed with `disposition`.
    pub fn kinds_with_disposition(&self, disposition: Disposition) -> Vec<CharacteristicKind> {
        let mut out: Vec<CharacteristicKind> = self
            .observations
            .iter()
            .filter(|o| o.disposition == disposition)
            .map(|o| o.kind)
            .collect();
        out.dedup();
        out
    }

    /// The [`SimultaneousRewriteDetail`] for phonological rule `rule`, if `rule` was observed as a
    /// `Simultaneous`-mode rewrite rule ([`SimultaneousSubruleOverlapPredicate`]'s own lookup).
    pub fn simultaneous_detail(&self, rule: PRuleId) -> Option<&SimultaneousRewriteDetail> {
        self.observations.iter().find_map(|o| match &o.detail {
            ObservationDetail::SimultaneousRewrite(d) if d.rule == rule => Some(d),
            _ => None,
        })
    }

    /// The grammar-wide [`MultiTableDetail`], if `g.char_tables.len() > 1` was observed at all
    /// ([`MultiTableFaithfulThreadingPredicate`]'s own lookup;
    /// `openspec/changes/fix-multitable-fst-compilation`).
    pub fn multi_table_detail(&self) -> Option<&MultiTableDetail> {
        self.observations.iter().find_map(|o| match &o.detail {
            ObservationDetail::MultiTable(d) => Some(d),
            _ => None,
        })
    }
}

// -------------------------------------------------------------------------------------------
// Private per-construct characterization helpers
// -------------------------------------------------------------------------------------------

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
/// literally every ordinary affixation grammar ever loaded, which is not what D1's "reduplication"
/// row means and would break this step's own "ordinary grammar characterizes Proven" test.
fn rhs_has_true_reduplication(rhs: &[OutputAction]) -> bool {
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

/// `true` iff `allo` has a multi-part LHS and its RHS never `Copy`s at least one of those parts —
/// i.e. real subtracted/discontinuous material (a circumfix wrapping the stem, or subtractive
/// morphology dropping a captured part outright). Independently re-derives the same structural
/// test `crate::emit::rhs_drops_lhs_material` uses for its own (unrelated: which compile ROUTE to
/// use) purpose — NOT imported (that fn is a private, unrelated-purpose helper in a module this
/// step must not modify), just the same well-defined predicate over the same frozen shapes.
///
/// This is deliberately NOT "does `allo.rhs` contain any `OutputAction` at all": every ordinary
/// concatenative affix (a ubiquitous `Copy(stem) + InsertSegments(affix)` shape) uses `OutputAction`
/// too, so flagging every occurrence would fail-close every ordinary affixation grammar — the same
/// over-triggering trap [`rhs_has_true_reduplication`]'s doc names for `ReduplicationHint`.
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

/// Exhaustively (no catch-all) matched per `OutputAction` occurrence (model.rs:686) purely for the
/// no-catch-all discipline itself — adding a fifth `OutputAction` variant breaks this build. Every
/// arm reports the same label-only outcome today: the real [`CharacteristicKind::
/// CircumfixOutputAction`] trigger is [`allomorph_drops_lhs_material`]'s structural test above, not
/// a per-variant capability difference (see that function's own doc for why "any `OutputAction` at
/// all" would over-trigger).
fn output_action_label(action: &OutputAction) -> &'static str {
    match action {
        OutputAction::Copy(_) => "copy",
        OutputAction::InsertSegments { .. } => "insert-segments",
        OutputAction::Modify(_, _) => "modify",
        OutputAction::InsertContext(_) => "insert-context",
    }
}

/// Exhaustively (no catch-all) matched per `CoOccurrenceAdjacency` value (model.rs:508) purely for
/// the discipline itself — every variant folds into the same [`CharacteristicKind::
/// CoOccurrenceConstraint`]/`ConfirmOnly` outcome (design.md D1's table: "(each) | co-occurrence
/// constraint | ConfirmOnly").
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

/// Computes [`MultiTableDetail`] for `g` (`fix-multitable-fst-compilation`): every pair of
/// distinct `char_tables` is checked for a shared normalized representation (any `<Representation>`
/// text any `CharDef` in one table claims, NFD-normalized exactly like
/// `pg_grammar::chardef::CharDefTable::lookup_nfd`'s own key) — `O(table_count^2 *
/// avg_table_size)`, cheap for any grammar in scope (table counts are small; this is a
/// characterization-time cost, not a per-word one). See
/// [`MultiTableFaithfulThreadingPredicate`]'s own doc for why pairwise representation-disjointness
/// is exactly the structural condition that makes per-rule table-correct resolution
/// (`pg_foma::replace::owning_table`) faithful with no residual cross-table token-collision risk.
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

/// Lowers one `Simultaneous`-mode subrule's D3 span via [`crate::lower::lower_span`], for
/// [`SubruleGateInfo::span`] — see that field's own doc for why this runs HERE (inside
/// `characterize`, which owns a live `&Grammar`) rather than inside
/// [`SimultaneousSubruleOverlapPredicate::evaluate`] itself.
///
/// Builds a fresh [`crate::replace::SegAlphabet`]/[`foma::options::FomaOptions`] per call rather
/// than threading them through `characterize`'s own signature — cheap (`SegAlphabet::new` only
/// borrows a table reference; `FomaOptions::default()` is a plain value struct), and keeps
/// `characterize`'s signature (`fn characterize(g: &Grammar) -> CharacteristicsProfile`, unchanged
/// since Step 1 of `add-capability-characteristics-check`) untouched.
///
/// Single-table assumption (`g.char_tables[0]`): this predicate's OWN documented gap, deliberately
/// UNCHANGED by `openspec/changes/fix-multitable-fst-compilation`. That change's scope is
/// `pg_foma::replace`'s rewrite-COMPILATION path (single-owner on `replace.rs`, its own tasks.md);
/// [`SimultaneousSubruleOverlapPredicate`] is a DIFFERENT construct (simultaneous-rewrite overlap,
/// owned by the future `compile-simultaneous-rewrites`), and several of this module's own minimal
/// unit fixtures for it deliberately declare a `<PhonologicalRule>` with NO `<Strata>` block at
/// all (no stratum to own it), which [`crate::replace::owning_table`] cannot resolve a table for
/// by construction. Also guarded (rather than indexed unchecked) so this never panics on a
/// `Grammar` with zero character tables at all.
fn lower_subrule_span(
    g: &Grammar,
    rule: &pg_grammar::model::RewriteRuleDef,
    sr: &pg_grammar::model::RewriteSubruleDef,
) -> LoweredSpan {
    let Some(table) = g.char_tables.first() else {
        return LoweredSpan::Unsupported(
            "grammar has no CharacterDefinitionTable at all; cannot lower any span".to_string(),
        );
    };
    let alphabet = crate::replace::SegAlphabet::new(table);
    let opts = foma::options::FomaOptions::default();
    match crate::lower::lower_span(
        &opts,
        g,
        &alphabet,
        sr.left_env.as_ref(),
        &rule.lhs,
        sr.right_env.as_ref(),
    ) {
        Ok((left, focus_right)) => LoweredSpan::Ok(Box::new((left, focus_right))),
        Err(reason) => LoweredSpan::Unsupported(reason.to_string()),
    }
}

fn characterize_allomorph(
    observations: &mut Vec<CharacteristicObservation>,
    rule: MRuleId,
    allomorph_index: usize,
    allo: &AffixAllomorphDef,
) {
    // Exhaustive `ReduplicationHint` match (model.rs:679), written unconditionally per allomorph
    // (not gated behind `rhs_has_true_reduplication` below) so a new variant breaks this build
    // regardless of whether any fixture exercises real reduplication.
    let _hint_label = match allo.redup_hint {
        ReduplicationHint::Prefix => "prefix",
        ReduplicationHint::Suffix => "suffix",
        ReduplicationHint::Implicit => "implicit",
    };
    if rhs_has_true_reduplication(&allo.rhs) {
        observations.push(CharacteristicObservation::new(
            CharacteristicKind::Reduplication,
            ModelLocation::AffixAllomorph {
                rule,
                allomorph_index,
            },
            ObservationDetail::None,
        ));
    }

    // Exhaustive `OutputAction` match (model.rs:686) per action, discipline-only (see
    // `output_action_label`'s own doc).
    for action in &allo.rhs {
        let _ = output_action_label(action);
    }
    if allomorph_drops_lhs_material(allo) {
        observations.push(CharacteristicObservation::new(
            CharacteristicKind::CircumfixOutputAction,
            ModelLocation::AffixAllomorph {
                rule,
                allomorph_index,
            },
            ObservationDetail::None,
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

/// D1's exhaustive default-deny characterizer: walks `g` and matches EVERY variant of EVERY
/// `model.rs` enum design.md D1 names, with no catch-all arm.
pub fn characterize(g: &Grammar) -> CharacteristicsProfile {
    let mut observations = Vec::new();

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
                // D5's first act: FailClosed, unconditionally, regardless of subrule shape.
                observations.push(CharacteristicObservation::new(
                    CharacteristicKind::Compounding,
                    ModelLocation::MorphRule(id),
                    ObservationDetail::None,
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

        // `AffixProcess`/`Realizational` share one allomorph shape (model.rs:576-590's own
        // uniform accessor) -- walk it once here rather than duplicating the match above.
        if let Some(allomorphs) = mrule.affix_allomorphs() {
            for (ai, allo) in allomorphs.iter().enumerate() {
                characterize_allomorph(&mut observations, id, ai, allo);
            }
        }

        // `CompoundingRule`'s own subrules carry `OutputAction`s too (model.rs:725) -- matched
        // exhaustively for the same discipline, but minting no NEW characteristic: `Compounding`
        // is already unconditionally `FailClosed` at the rule level above (D5's first act), so a
        // per-subrule `CircumfixOutputAction`/`Reduplication` observation here would be redundant,
        // not more faithful.
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
                ObservationDetail::None,
            )),
        }
    }

    // --- MprGroup / MprGroupOutput (model.rs:824-842) ---------------------------------------
    for (i, group) in g.mpr_groups.iter().enumerate() {
        // `MprGroupMatchType` (model.rs:825) has no disposition of its own in D1's table (only
        // `MprGroupOutput` does) -- matched exhaustively anyway, no-op, purely so a third
        // match-type variant is forced through this file rather than silently ignored.
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
                                span: lower_subrule_span(g, r, sr),
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
                    Dir::RightToLeft => observations.push(CharacteristicObservation::new(
                        CharacteristicKind::RightToLeftRewrite,
                        ModelLocation::PhonRule(id),
                        ObservationDetail::None,
                    )),
                }
                // "Epenthesis" (D1) is an empty-`lhs` RULE, not a subrule field (model.rs:417's
                // own doc: "empty pattern if absent (epenthesis rules)" is on `RewriteRuleDef.lhs`
                // -- design.md's table cites `RewriteSubruleDef` for this row, but the frozen
                // model actually carries it one level up; flagged for review, not silently
                // "corrected" without note).
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
            }
            PhonRuleDef::Metathesis(_) => observations.push(CharacteristicObservation::new(
                CharacteristicKind::Metathesis,
                ModelLocation::PhonRule(id),
                ObservationDetail::None,
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

    // --- AllomorphCoOccurrenceRuleDef (model.rs:508/535), on ROOT allomorphs ----------------
    // (affix-allomorph co-occurrence is already covered inside `characterize_allomorph` above).
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
        }
    }

    // --- Grammar-level: Grammar::char_tables.len() > 1 (model.rs:1100) ----------------------
    // (`fix-multitable-fst-compilation`). Attributed to the FIRST stratum whose own table
    // differs from the base (stratum 0's) table -- a real `ModelLocation`, not a synthetic one,
    // while the actual DETAIL below is grammar-wide (every table pair, not just that one stratum).
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

// =================================================================================================
// D2: CapabilityPredicate + PredicateVerdict + EvidenceProvenance + CapabilityDiagnostic
// =================================================================================================

/// A predicate's stable identity (design.md D2: `"simultaneous.subrule-overlap"`, etc.).
pub type PredicateId = &'static str;

/// Where a predicate's evidence comes from (ADR 0001, design.md D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceProvenance {
    /// Evidence comes from testing black-box behavior (e.g. foma `apply_up`/`apply_down` oracle
    /// witnesses) — automata are not directly inspectable on this path.
    Behavioral,
    /// Evidence comes from directly inspecting compositional structure (lowered automata, or —
    /// as [`SimultaneousSubruleOverlapPredicate`] does today — directly-readable model fields like
    /// `required_mpr`/`excluded_mpr`/`self_opaquing`).
    Structural,
}

/// A `Refuse` verdict's typed payload: which predicate refused, what construct/config, and a
/// human-readable witness (design.md's scenario: "compilation fails... with a typed diagnostic
/// naming the construct and configuration").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityDiagnostic {
    pub predicate: PredicateId,
    pub construct: String,
    pub witness: String,
}

/// A capability predicate's verdict for one plan node (design.md D2, exactly).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PredicateVerdict {
    /// Proven faithful; admission-filtering allowed.
    Admit,
    /// Propose the superset; no no-false-negative proof. First-class, not a failure (ADR 0001).
    ConfirmOnly,
    /// Hard compile-time fail (overridable per ADR 0005).
    Refuse(CapabilityDiagnostic),
}

/// design.md D2, exactly: an oracle-verified, conservative proof obligation. Implementors MUST
/// over-refuse rather than under-refuse — the discipline every predicate in this module follows.
pub trait CapabilityPredicate {
    /// e.g. `"simultaneous.subrule-overlap"`.
    fn id(&self) -> PredicateId;
    /// Which [`CharacteristicKind`](s) this predicate claims to discharge.
    fn discharges(&self) -> &[CharacteristicKind];
    /// This predicate's verdict for `plan_node`, given the grammar-wide `profile` (see this
    /// module's own top-doc for why `plan_node: &PlanNodeKind` rather than D2's literal
    /// `&PlanNode` — `crate::plan` has no type by that name).
    fn evaluate(&self, profile: &CharacteristicsProfile, plan_node: &PlanNodeKind)
        -> PredicateVerdict;
    /// [`EvidenceProvenance::Behavioral`] or [`EvidenceProvenance::Structural`].
    fn provenance(&self) -> EvidenceProvenance;
}

// -------------------------------------------------------------------------------------------
// D3: the worked simultaneous.subrule-overlap predicate
// -------------------------------------------------------------------------------------------

/// Extracts the [`PRuleId`] a rewrite-rule [`PlanNodeKind::Leaf`] is addressed by, if `plan_node`
/// is one. Any other node shape (this predicate is only meaningful at a rewrite-rule leaf) yields
/// `None`, which [`SimultaneousSubruleOverlapPredicate::evaluate`] treats as "not applicable here,"
/// i.e. vacuously `Admit` — not a capability judgment, just "wrong node kind to ask this predicate
/// about."
fn rewrite_rule_of(plan_node: &PlanNodeKind) -> Option<PRuleId> {
    match plan_node {
        PlanNodeKind::Leaf {
            fragment: FragmentSpec::RewriteRule { rule },
            ..
        } => Some(*rule),
        _ => None,
    }
}

/// Design.md D3's "cheap orthogonality early-out": `true` iff NO lexical entry can ever satisfy
/// both `a` and `b`'s MPR gates simultaneously, because one subrule REQUIRES an MPR feature the
/// other EXCLUDES (in either direction). This is a SUFFICIENT, not necessary, condition for
/// disjointness — deliberately conservative in the safe direction: it can miss some genuinely
/// disjoint pairs (falling through to the stricter overlap test below, never to a wrong `Admit`),
/// but it can never wrongly call two OVERLAPPING gates disjoint (each direction is a direct MprSet
/// containment fact, not a heuristic).
fn mpr_gates_disjoint(a: &SubruleGateInfo, b: &SubruleGateInfo) -> bool {
    a.required_mpr.overlaps(b.excluded_mpr) || b.required_mpr.overlaps(a.excluded_mpr)
}

/// The worked example (design.md D3, ADR 0001's cited case): a `RewriteRuleDef` with `mode ==
/// Simultaneous` is faithfully compilable UNLESS two of its subrules' environments can match at
/// the same input position.
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
/// # The real automaton intersection (Stage 1B, `lower-fst-pattern-environments`)
/// D3's precise test is `intersect(span(s_i), span(s_j))` where `span(s) = left_env · lhs_focus ·
/// right_env`, lowered to an `Fsm` via [`crate::lower::lower_span`]. That facility now exists
/// (Stage 1B landed alongside this predicate's own upgrade): every pair that survives the
/// `self_opaquing`/`mpr_gates_disjoint` early-outs is decided by
/// [`crate::lower::spans_overlap`] over each subrule's [`SubruleGateInfo::span`] (pre-lowered by
/// [`characterize`] — see [`LoweredSpan`]'s own doc for why lowering happens THERE, not in this
/// `evaluate` call). `Refuse` only when the intersection is genuinely NON-EMPTY (a real witness
/// overlap), or when either span's [`LoweredSpan`] is [`LoweredSpan::Unsupported`] (a pattern node
/// kind `lower_span` cannot yet represent — D3's own words, "any approximation rounds toward
/// Refuse," still applies to THAT residual gap). This `Admit`s strictly more pairs than the prior
/// unconditional-`Refuse` fallback did (never fewer — over-refusal only ever narrows as proof
/// machinery improves, per ADR 0001); see this module's test module for a pair that was
/// `Refuse`-only before this step and is now proven `Admit`.
///
/// # Provenance
/// [`EvidenceProvenance::Structural`]: `self_opaquing`/`mpr_gates_disjoint` still read directly-
/// inspectable `model.rs` fields for their own early-outs, and the surviving-pair test now
/// genuinely intersects REAL lowered automata (`crate::lower`) — exactly the "controllable
/// composition path" design.md D3 reserves `Structural` for, no longer a judgment call: this is no
/// longer evidence-kind-matches-but-proof-not-yet-built (the prior step's own caveat), it now IS
/// that controllable-composition proof.
pub struct SimultaneousSubruleOverlapPredicate;

impl CapabilityPredicate for SimultaneousSubruleOverlapPredicate {
    fn id(&self) -> PredicateId {
        "simultaneous.subrule-overlap"
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::SimultaneousRewrite]
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        profile: &CharacteristicsProfile,
        plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        let Some(rule) = rewrite_rule_of(plan_node) else {
            return PredicateVerdict::Admit;
        };
        let Some(detail) = profile.simultaneous_detail(rule) else {
            // Not observed as a Simultaneous rule at all (e.g. it's Iterative) -- Iterative is
            // Proven (D1), this predicate has nothing to say about it.
            return PredicateVerdict::Admit;
        };

        for i in 0..detail.subrules.len() {
            for j in (i + 1)..detail.subrules.len() {
                let a = &detail.subrules[i];
                let b = &detail.subrules[j];

                // D3: "if either subrule is self_opaquing, do not attempt Admit" -- checked BEFORE
                // the mpr-gate early-out, unconditionally.
                if a.self_opaquing || b.self_opaquing {
                    return PredicateVerdict::Refuse(CapabilityDiagnostic {
                        predicate: self.id(),
                        construct: format!(
                            "prule {} subrules {}/{}",
                            rule.0, a.index, b.index
                        ),
                        witness: format!(
                            "subrule {} and/or {} is self_opaquing (analysis fixpoint reapply); \
                             D3 rounds any self-opaquing pair to Refuse rather than attempt Admit",
                            a.index, b.index
                        ),
                    });
                }

                if mpr_gates_disjoint(a, b) {
                    continue;
                }

                // Stage 1B: the real automaton intersection (see this type's own doc). Either
                // span being Unsupported rounds to Refuse (D3: "any approximation rounds toward
                // Refuse"), naming the unhandled construct rather than silently admitting it.
                let opts = foma::options::FomaOptions::default();
                match (&a.span, &b.span) {
                    (LoweredSpan::Ok(span_a), LoweredSpan::Ok(span_b)) => {
                        let overlaps = crate::lower::spans_overlap(&opts, span_a, span_b);
                        if overlaps {
                            return PredicateVerdict::Refuse(CapabilityDiagnostic {
                                predicate: self.id(),
                                construct: format!(
                                    "prule {} subrules {}/{}",
                                    rule.0, a.index, b.index
                                ),
                                witness: format!(
                                    "subrules {} and {} are not mpr-gate-disjoint, and their \
                                     lowered left_env/lhs_focus/right_env spans (Stage 1B, \
                                     crate::lower) genuinely intersect at a shared focus \
                                     position -- a real overlap witness, not an unproven \
                                     approximation",
                                    a.index, b.index
                                ),
                            });
                        }
                        // Proven non-overlapping: fall through to the next pair.
                    }
                    (LoweredSpan::Unsupported(reason), _) | (_, LoweredSpan::Unsupported(reason)) => {
                        return PredicateVerdict::Refuse(CapabilityDiagnostic {
                            predicate: self.id(),
                            construct: format!(
                                "prule {} subrules {}/{}",
                                rule.0, a.index, b.index
                            ),
                            witness: format!(
                                "subrules {} and {} are not mpr-gate-disjoint, and at least one \
                                 span could not be lowered (Stage 1B, crate::lower): {reason}; \
                                 conservatively rounding toward overlap-possible",
                                a.index, b.index
                            ),
                        });
                    }
                }
            }
        }

        PredicateVerdict::Admit
    }
}

// -------------------------------------------------------------------------------------------
// MultiTable: the config-predicate `fix-multitable-fst-compilation` registers
// -------------------------------------------------------------------------------------------

/// `openspec/changes/fix-multitable-fst-compilation`'s own capability predicate: a `Grammar` with
/// more than one `CharacterDefinitionTable` (D1's `MultiTable` characteristic) is faithfully
/// compilable by `pg_foma::replace` now that every rewrite rule resolves its own natural
/// classes/alpha variables against ITS OWNING stratum's table (`owning_table`, never an implicit
/// `char_tables[0]` default — this change's whole `replace.rs`/`lower.rs` fix), PROVIDED no two
/// tables share a literal representation (spelling).
///
/// # Why representation-disjointness is the proof obligation, not just "the fix landed"
/// `pg_foma::replace::SegAlphabet::token` is (and remains, unchanged by this fix) a PURE function
/// of a `CharDefId`'s raw per-table index (`PUA_BASE + cd.0`), not of which table that index came
/// from. Threading each RULE to its own correct table (this change's fix) makes every rule's OWN
/// natural-class/alpha resolution correct in isolation, but a composed cascade that mixes material
/// from TWO tables (e.g. a root spelled via table A's lexicon flowing into a later stratum's
/// table-B-resolved rule) could still, in principle, let table B's rule accidentally match a
/// table-A-originated token that merely shares the same RAW index as one of table B's own
/// segments — UNLESS the two tables' own character inventories are disjoint (no shared spelling),
/// in which case no root/affix material ever legitimately carries the "other" table's tokens in
/// the first place, so the collision is structurally unreachable. This predicate's own
/// [`multi_table_detail`] computes exactly that pairwise check.
///
/// # Disposition
/// - **Zero or one table observed at all:** vacuously `Admit` (this predicate has nothing to say —
///   [`Disposition::Proven`] already covers the ordinary single-table case, D1's own resting
///   disposition for every characteristic the grammar never exercises).
/// - **Pairwise-disjoint tables:** [`PredicateVerdict::ConfirmOnly`] — per-rule table-correct
///   resolution is now faithful (this change's fix + the disjointness argument above rule out the
///   residual token-collision risk), but no PROVEN no-false-positive admission-filter argument
///   exists yet (ADR 0001's own bar for `Admit`), so this is confirm-only-by-default, not `Admit`.
///   The oracle (`pg_rules::rewrite`, which resolves every rule's table via an explicit `TableId`
///   parameter with no PUA-token collapsing at all) prunes any residual over-generation the P6
///   proposer's shared token space might still admit.
/// - **Tables share a representation:** [`PredicateVerdict::Refuse`] — the residual case this
///   change's threading fix cannot make faithful (module doc above); conservative, overridable per
///   ADR 0005, never a silent wrong compile.
///
/// # Provenance
/// [`EvidenceProvenance::Structural`]: `multi_table_detail`'s pairwise-representation check reads
/// directly-inspectable `CharDefTable`/`CharDef` data, no oracle witnesses needed to derive it (the
/// oracle IS still what discharges the `ConfirmOnly` verdict's own recall obligation, per the
/// module doc above, but the PREDICATE's own verdict is a structural fact about the tables
/// themselves).
///
/// # Node applicability
/// Grammar-wide, not node-specific (same shape as the `FailClosedPlaceholder`s this module's own
/// `compose_envelope` doc names as having "no corresponding `PlanNodeKind`" — `MultiTable`'s own
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

    fn discharges(&self) -> &[CharacteristicKind] {
        &[CharacteristicKind::MultiTable]
    }

    fn provenance(&self) -> EvidenceProvenance {
        EvidenceProvenance::Structural
    }

    fn evaluate(
        &self,
        profile: &CharacteristicsProfile,
        _plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        let Some(detail) = profile.multi_table_detail() else {
            // Not observed at all (<= 1 table) -- nothing for this predicate to say (module doc).
            return PredicateVerdict::Admit;
        };
        if !detail.representations_pairwise_disjoint {
            return PredicateVerdict::Refuse(CapabilityDiagnostic {
                predicate: self.id(),
                construct: format!("{} character-definition tables", detail.table_count),
                witness: detail
                    .shared_representation_witness
                    .clone()
                    .unwrap_or_else(|| {
                        "two tables share a representation (witness not captured)".to_string()
                    }),
            });
        }
        PredicateVerdict::ConfirmOnly
    }
}

// =================================================================================================
// The predicate registry (design.md D2's "no silent vacuous pass" coverage check)
// =================================================================================================

/// A placeholder [`CapabilityPredicate`] for a `FailClosed`/`ConfigPredicate` characteristic that
/// has no real predicate implemented yet (every characteristic besides
/// [`CharacteristicKind::SimultaneousRewrite`], as of this step). Unconditionally `Refuse`s —
/// correct under this module's conservative discipline (over-refuse is always safe), and exists
/// only so [`undischarged_kinds`] can report TRUE 100% coverage of the registry contract rather
/// than a coverage check that itself has silent gaps. `owning_change` names the Stage-2 OpenSpec
/// change expected to replace this placeholder with a real per-construct predicate.
pub struct FailClosedPlaceholder {
    id: PredicateId,
    discharges: Vec<CharacteristicKind>,
    owning_change: &'static str,
}

impl FailClosedPlaceholder {
    pub fn new(
        id: PredicateId,
        discharges: &[CharacteristicKind],
        owning_change: &'static str,
    ) -> Self {
        FailClosedPlaceholder {
            id,
            discharges: discharges.to_vec(),
            owning_change,
        }
    }
}

impl CapabilityPredicate for FailClosedPlaceholder {
    fn id(&self) -> PredicateId {
        self.id
    }

    fn discharges(&self) -> &[CharacteristicKind] {
        &self.discharges
    }

    fn provenance(&self) -> EvidenceProvenance {
        // No real evidence is gathered at all -- this is a stub, not a proof.
        EvidenceProvenance::Behavioral
    }

    fn evaluate(
        &self,
        _profile: &CharacteristicsProfile,
        _plan_node: &PlanNodeKind,
    ) -> PredicateVerdict {
        PredicateVerdict::Refuse(CapabilityDiagnostic {
            predicate: self.id,
            construct: format!("{:?}", self.discharges),
            witness: format!(
                "no real predicate implemented yet (Step 1 of add-capability-characteristics-check \
                 registers only a conservative placeholder here); owning Stage-2 change: {}",
                self.owning_change
            ),
        })
    }
}

/// A collection of [`CapabilityPredicate`]s, queryable for whether a [`CharacteristicKind`] is
/// discharged by at least one of them (design.md D2's coverage requirement).
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

/// The minimal registry this step ships: the two REAL predicates
/// ([`SimultaneousSubruleOverlapPredicate`], [`MultiTableFaithfulThreadingPredicate`]), plus an
/// explicit [`FailClosedPlaceholder`] for every other `FailClosed`/`ConfigPredicate` characteristic
/// — proving the coverage contract holds today without pretending any of those other constructs
/// has a real proof yet.
pub fn default_registry() -> PredicateRegistry {
    let mut r = PredicateRegistry::new();
    r.register(Box::new(SimultaneousSubruleOverlapPredicate));
    r.register(Box::new(MultiTableFaithfulThreadingPredicate));
    r.register(Box::new(FailClosedPlaceholder::new(
        "compounding.placeholder",
        &[CharacteristicKind::Compounding],
        "cover-compounding",
    )));
    r.register(Box::new(FailClosedPlaceholder::new(
        "unordered-morph-rules.placeholder",
        &[CharacteristicKind::UnorderedMorphRuleApplication],
        "cover-unordered-morph-rules",
    )));
    r.register(Box::new(FailClosedPlaceholder::new(
        "mpr-group-overwrite.placeholder",
        &[CharacteristicKind::MprGroupOverwrite],
        "cover-mpr-groups",
    )));
    r.register(Box::new(FailClosedPlaceholder::new(
        "right-to-left-rewrite.placeholder",
        &[CharacteristicKind::RightToLeftRewrite],
        "compile-right-to-left-rewrites",
    )));
    r.register(Box::new(FailClosedPlaceholder::new(
        "metathesis.placeholder",
        &[CharacteristicKind::Metathesis],
        "compile-fst-metathesis",
    )));
    r.register(Box::new(FailClosedPlaceholder::new(
        "epenthesis.placeholder",
        &[CharacteristicKind::Epenthesis],
        // design.md D1 names no `cover-*` change for this row -- flagged for review.
        "TODO: no owning Stage-2 change named by design.md yet for epenthesis",
    )));
    r.register(Box::new(FailClosedPlaceholder::new(
        "circumfix-output-action.placeholder",
        &[CharacteristicKind::CircumfixOutputAction],
        "cover-circumfix-null-output-actions",
    )));
    r.register(Box::new(FailClosedPlaceholder::new(
        "reduplication.placeholder",
        &[CharacteristicKind::Reduplication],
        // Reduplication-peel is owned by cover-template-truncation-reduplication (STAGING Stage 2
        // item 7; wired as an ADR 0004 required-runtime-feature + ADR 0003 chain-depth apply op).
        "cover-template-truncation-reduplication",
    )));
    r
}

/// design.md D2 / spec.md's "no silent vacuous pass" requirement: every [`CharacteristicKind`]
/// whose [`CharacteristicKind::default_disposition`] is [`Disposition::FailClosed`] or
/// [`Disposition::ConfigPredicate`] must be named by at least one registered predicate's
/// [`CapabilityPredicate::discharges`]. Returns the undischarged kinds (empty iff `registry` is
/// complete) rather than a bool, so a failing check can report exactly what's missing.
pub fn undischarged_kinds(registry: &PredicateRegistry) -> Vec<CharacteristicKind> {
    CharacteristicKind::ALL
        .iter()
        .copied()
        .filter(|k| {
            matches!(
                k.default_disposition(),
                Disposition::FailClosed | Disposition::ConfigPredicate
            )
        })
        .filter(|k| !registry.discharges(*k))
        .collect()
}

// =================================================================================================
// D4: bottom-up envelope composition + the CHECK-ONLY compile decision (Step 2)
// =================================================================================================

/// The overall, whole-plan CHECK-ONLY compile decision [`compose_envelope`] returns (design.md D4;
/// spec.md: "A node verdict SHALL be the meet of its children's verdicts and its own predicate,
/// with Refuse dominating and any ConfirmOnly demoting the subtree"). Distinct from
/// [`PredicateVerdict`] (D2's PER-PREDICATE, single-node verdict, carrying at most one
/// [`CapabilityDiagnostic`]): composing a whole plan can collect refusals from many different
/// nodes/observations, and a caller should see all of them, not just whichever one [`meet`] folded
/// in first — this type widens the single diagnostic to a deduplicated `Vec` at exactly the point
/// those per-node/per-observation verdicts get folded together.
///
/// **CHECK-ONLY** (this module's own top-doc "D4 (Step 2)" section): nothing in this crate
/// consults a [`CompileDecision`] to block or alter any real compile path yet. That wiring — the
/// production flip, ADR 0005's override, and the CI cross-check — is later `tasks.md` work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileDecision {
    /// Every construct in the plan is `Proven`, or has a predicate-proven [`PredicateVerdict::Admit`].
    /// Admission-filtering is licensed.
    Admit,
    /// At least one construct rests at (or was proven no better than) `ConfirmOnly`, and NONE is
    /// refused. Propose the superset, no admission-filtering — first-class, not a failure (ADR
    /// 0001).
    ConfirmOnly,
    /// At least one construct is refused. Carries EVERY [`CapabilityDiagnostic`] collected while
    /// composing the plan (content-deduplicated — see [`meet`]'s own doc), not just the first, so a
    /// caller sees every problem in one pass rather than one compile attempt at a time.
    Refuse(Vec<CapabilityDiagnostic>),
}

/// D4's lattice, made explicit: `Refuse` dominates `ConfirmOnly` dominates `Admit`/`Proven` —
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
/// deduplicated: the same [`CapabilityDiagnostic`] can be reached via two different DAG paths to a
/// shared node (D1's content-addressed sharing means a single offending leaf can be a descendant of
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

/// Widens one predicate's [`PredicateVerdict`] (D2: one diagnostic max) into a [`CompileDecision`]
/// (this section: a `Vec` of diagnostics) so it can be [`meet`]-folded together with other nodes'/
/// observations' decisions.
fn verdict_to_decision(verdict: PredicateVerdict) -> CompileDecision {
    match verdict {
        PredicateVerdict::Admit => CompileDecision::Admit,
        PredicateVerdict::ConfirmOnly => CompileDecision::ConfirmOnly,
        PredicateVerdict::Refuse(diag) => CompileDecision::Refuse(vec![diag]),
    }
}

/// The overall decision floor for an observed, non-`Proven` [`CharacteristicKind`] that NO
/// registered predicate discharges at all — there is no `evaluate` call to make for it, only
/// `kind`'s own default [`Disposition`] to fold in directly (design.md's D1 table, restated as a
/// [`CompileDecision`]).
///
/// - [`Disposition::ConfirmOnly`]/[`Disposition::ConfigPredicate`] rest at
///   [`CompileDecision::ConfirmOnly`] absent a predicate proving `Admit` — exactly
///   [`Disposition::ConfigPredicate`]'s own doc ("`ConfirmOnly` unless/until a registered predicate
///   proves `Admit`") and [`Disposition::ConfirmOnly`]'s own doc (recall-preserving only if the
///   proposer proposes the superset — never promotable to `Admit` without a proof this function has
///   no predicate to supply). This is the landing spot for e.g. an observed
///   [`CharacteristicKind::MprGroupAppend`]: [`default_registry`] intentionally registers no
///   predicate for it at all (`ConfirmOnly` already IS its resting disposition, per D1's table —
///   there is nothing to prove up to `Admit` and no coverage gap either, since
///   [`undischarged_kinds`] only requires coverage for `FailClosed`/`ConfigPredicate` kinds).
/// - [`Disposition::FailClosed`] with NO discharging predicate registered at all is a REGISTRY
///   COVERAGE GAP ([`undischarged_kinds`] exists precisely to catch this at the registry level, and
///   [`default_registry`]'s own test proves it never happens for that registry). Handled here
///   defensively for any OTHER caller-supplied [`PredicateRegistry`] that omits it, by folding in a
///   synthetic `Refuse` naming the gap rather than silently `Admit`ting an unproven-by-construction
///   characteristic — the exact failure mode ADR 0001 forbids.
/// - [`Disposition::Proven`] never actually reaches this function in practice (callers only invoke
///   it for observations already filtered to `disposition != Proven`); matched here anyway for the
///   same no-catch-all discipline the rest of this module holds itself to.
fn disposition_floor(kind: CharacteristicKind, disposition: Disposition) -> CompileDecision {
    match disposition {
        Disposition::Proven => CompileDecision::Admit,
        Disposition::ConfigPredicate | Disposition::ConfirmOnly => CompileDecision::ConfirmOnly,
        Disposition::FailClosed => CompileDecision::Refuse(vec![CapabilityDiagnostic {
            predicate: "registry-coverage-gap.no-predicate-registered",
            construct: format!("{kind:?}"),
            witness: format!(
                "{kind:?} is FailClosed by default disposition (design.md D1) but the supplied \
                 PredicateRegistry registers no predicate discharging it -- conservatively \
                 refusing rather than silently admitting an unproven-by-construction \
                 characteristic (run undischarged_kinds() against any production registry to catch \
                 this earlier)"
            ),
        }]),
    }
}

/// Computes `node_id`'s bottom-up [`CompileDecision`] within `plan` (design.md D4: "a node's
/// verdict is the meet of its children's verdicts and its own node-level predicate"), memoized by
/// [`NodeId`] in `cache` so a node shared by multiple parents (D1's content-addressed DAG sharing)
/// is evaluated exactly once, not once per parent referencing it.
///
/// A node's "own predicate verdicts" are every predicate in `registry` whose
/// [`CapabilityPredicate::discharges`] names a [`CharacteristicKind`] present in `relevant_kinds`
/// (every kind `compose_envelope` found the profile observed with a non-`Proven` disposition). This
/// guard exists because [`FailClosedPlaceholder`]'s `evaluate` ignores both `profile` and
/// `plan_node` and unconditionally `Refuse`s — calling it at every node of every plan regardless of
/// whether its characteristic was ever observed would force every grammar to `Refuse`, including
/// ordinary ones that never exercise that construct at all. Gating on "was this kind observed
/// anywhere" makes a predicate a pure no-op at every node when its construct genuinely does not
/// occur in this grammar, which is always safe (there is nothing to refuse if the construct is
/// absent) — never a shortcut that could skip a predicate whose construct actually IS present.
///
/// A predicate whose construct DOES occur (e.g. [`SimultaneousSubruleOverlapPredicate`]) is still
/// called at literally EVERY node the walk visits, not just the "right" one — correctness relies on
/// well-behaved predicates already being self-gating on node applicability (D2's own contract:
/// `evaluate` "may return `Refuse` too eagerly, never `Admit` too eagerly", and
/// [`SimultaneousSubruleOverlapPredicate::evaluate`]'s own early `Admit` returns for a non-
/// `RewriteRule` leaf, or a `RewriteRule` leaf whose `PRuleId` isn't the observed `Simultaneous`
/// rule), not on this function pre-filtering by node shape. This is also exactly how a
/// [`CharacteristicKind::SimultaneousRewrite`] observation's [`ModelLocation::PhonRule`] gets
/// "mapped" onto its plan node: `crate::enumerate::enumerate_default` mints one
/// `Leaf { fragment: FragmentSpec::RewriteRule { rule }, .. }` per `PRuleId`, the walk below visits
/// every leaf, and the predicate's own `PRuleId`-keyed lookup
/// ([`CharacteristicsProfile::simultaneous_detail`]) does the actual matching — no separate
/// `ModelLocation -> NodeId` lookup table is built, because the walk already provides it.
fn node_decision(
    plan: &Plan,
    profile: &CharacteristicsProfile,
    registry: &PredicateRegistry,
    relevant_kinds: &HashSet<CharacteristicKind>,
    node_id: NodeId,
    cache: &mut HashMap<NodeId, CompileDecision>,
) -> CompileDecision {
    if let Some(cached) = cache.get(&node_id) {
        return cached.clone();
    }
    // A dangling id (not interned in this plan) would be a caller/plan-construction bug, not a
    // capability judgment -- fold in as vacuously Admit rather than panic, since this function's
    // job is a conservative DECISION over a well-formed Plan, not plan validation (`crate::plan`
    // owns that).
    let Some(kind) = plan.get(node_id) else {
        return CompileDecision::Admit;
    };

    let mut decision = CompileDecision::Admit;
    for &child in kind.children() {
        decision = meet(
            decision,
            node_decision(plan, profile, registry, relevant_kinds, child, cache),
        );
    }
    for predicate in registry.predicates() {
        if predicate
            .discharges()
            .iter()
            .any(|k| relevant_kinds.contains(k))
        {
            decision = meet(decision, verdict_to_decision(predicate.evaluate(profile, kind)));
        }
    }

    cache.insert(node_id, decision.clone());
    decision
}

/// Step 2 of `add-capability-characteristics-check` (design.md D4): composes the capability
/// envelope bottom-up over `plan` (the reified compilation plan `crate::enumerate::
/// enumerate_default` builds) and returns the overall CHECK-ONLY [`CompileDecision`] — connecting
/// Step 1's two spines, [`characterize`] (the profile) and `enumerate_default` (the plan), through
/// `registry`.
///
/// # Algorithm (design.md D4, spec.md)
/// 1. [`characterize`] projects `g` into a [`CharacteristicsProfile`].
/// 2. Every observed [`CharacteristicKind`] whose disposition is NOT [`Disposition::Proven`] is
///    collected into a `relevant_kinds` set ([`node_decision`]'s own doc explains why).
/// 3. `plan`'s root is walked bottom-up via [`node_decision`]: the meet of every node's children's
///    decisions and its own applicable registered predicates.
/// 4. Separately, every OBSERVED non-`Proven` kind that NO registered predicate discharges at all
///    (so step 3 never had an `evaluate` call to make for it — e.g. [`CharacteristicKind::
///    MprGroupAppend`], which [`default_registry`] intentionally leaves undischarged since
///    `ConfirmOnly` is already its own resting disposition) is folded in via [`disposition_floor`],
///    so a grammar-wide characteristic with no registered predicate at all still pulls the overall
///    decision down.
/// 5. The two folds [`meet`] into the final, overall [`CompileDecision`].
///
/// # Judgment call: constructs with no distinct plan node
/// Several `FailClosed`/`ConfigPredicate` characteristics ([`CharacteristicKind::Compounding`],
/// [`CharacteristicKind::UnorderedMorphRuleApplication`], [`CharacteristicKind::MprGroupOverwrite`],
/// [`CharacteristicKind::Epenthesis`], [`CharacteristicKind::CircumfixOutputAction`],
/// [`CharacteristicKind::Reduplication`]) have NO corresponding [`crate::plan::PlanNodeKind`] in
/// today's `enumerate_default` shape at all — that module's own doc: it only ever mints leaves for
/// the lexicon (per gate group), one per rewrite rule, and the two composite-emission markers,
/// nothing addressed by `MRuleId`/`StratumId`/an mpr-group index. Each of these characteristics is
/// discharged today by a [`FailClosedPlaceholder`], whose `evaluate` unconditionally `Refuse`s
/// REGARDLESS of which node it is called at or what `profile` says (that type's own Step-1 doc) —
/// so which specific node it is evaluated against is behaviorally irrelevant here, and
/// [`node_decision`]'s per-node walk (which calls every relevant-kind predicate at EVERY node)
/// already folds its `Refuse` in correctly without needing a `ModelLocation -> NodeId` lookup table
/// for these kinds at all. This is this step's "representative node" case: no lookup was built
/// because none would change the outcome, not because one was skipped for convenience — documented
/// here rather than silently. [`CharacteristicKind::SimultaneousRewrite`] is the one kind that DOES
/// need (and gets, via the plan walk itself) a SPECIFIC node — see [`node_decision`]'s own doc for
/// how that mapping actually happens.
pub fn compose_envelope(g: &Grammar, plan: &Plan, registry: &PredicateRegistry) -> CompileDecision {
    let profile = characterize(g);
    let relevant_kinds: HashSet<CharacteristicKind> = profile
        .observations()
        .iter()
        .filter(|o| o.disposition != Disposition::Proven)
        .map(|o| o.kind)
        .collect();

    let mut cache = HashMap::new();
    let mut decision = match plan.root() {
        Some(root) => node_decision(plan, &profile, registry, &relevant_kinds, root, &mut cache),
        None => CompileDecision::Admit,
    };

    for &kind in &relevant_kinds {
        if !registry.discharges(kind) {
            decision = meet(decision, disposition_floor(kind, kind.default_disposition()));
        }
    }

    decision
}

#[cfg(test)]
mod tests {
    //! Synthetic, delanguaged fixtures only (no natural-language names) -- built via
    //! `pg_grammar::load` from hand-authored XML, mirroring `gate.rs`'s own test-module style
    //! rather than hand-constructing a `Grammar` (which would require standing up every interner
    //! field by hand; `load` is this workspace's own supported entry point for exactly this).

    use pg_grammar::model::{MorphRuleDef, PhonRuleDef, PRuleId};

    use super::*;
    use crate::enumerate::enumerate_default;
    use crate::junctions::PhonologyProbe;
    use crate::plan::{FragmentSpec, PlanNodeKind, Provenance};
    use crate::replace::SegAlphabet;

    fn load(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    /// `crate::enumerate::enumerate_default`'s own test-module helper, duplicated here (not
    /// shared -- test modules don't share private helpers across files): the grammar's
    /// phonological rules in cascade order, as literal borrows of `g.prules` (required for
    /// `enumerate_default`'s pointer-identity `PRuleId` recovery -- see `enumerate::rule_id_of`'s
    /// own doc).
    fn prules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
        g.strata
            .iter()
            .flat_map(|s| &s.prules)
            .map(|&id| &g.prules[id.0 as usize])
            .collect()
    }

    /// Builds `g`'s enumerated [`crate::plan::Plan`] via the REAL `enumerate_default` seam (Step 2
    /// of `reify-compilation-plans`), exactly the way a real caller would -- these
    /// `compose_envelope` tests exercise the full characterize+enumerate+compose pipeline end to
    /// end, not a hand-built `Plan`.
    fn enumerated_plan(g: &Grammar) -> Plan {
        let alphabet = SegAlphabet::new(&g.char_tables[0]);
        let ro = prules_in_order(g);
        let phon = PhonologyProbe::new(g);
        enumerate_default(g, &alphabet, &ro, phon.as_ref())
    }

    // ---------------------------------------------------------------------------------------
    // characterize(): FailClosed triggers
    // ---------------------------------------------------------------------------------------

    /// D5's first act: `MorphRuleDef::Compounding` characterizes FailClosed.
    #[test]
    fn characterize_marks_compounding_fail_closed() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        assert!(matches!(g.mrules[0], MorphRuleDef::Compounding(_)));

        let profile = characterize(&g);
        assert!(
            profile
                .observations()
                .iter()
                .any(|o| o.kind == CharacteristicKind::Compounding
                    && o.disposition == Disposition::FailClosed),
            "Compounding must characterize FailClosed: {:?}",
            profile.observations()
        );
    }

    /// D5's first act: `MorphRuleOrder::Unordered` characterizes FailClosed.
    #[test]
    fn characterize_marks_unordered_morph_rule_order_fail_closed() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
              <Name>S</Name>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        assert_eq!(g.strata[0].mrule_order, MorphRuleOrder::Unordered);

        let profile = characterize(&g);
        assert!(
            profile.observations().iter().any(|o| o.kind
                == CharacteristicKind::UnorderedMorphRuleApplication
                && o.disposition == Disposition::FailClosed),
            "Unordered stratum must characterize FailClosed: {:?}",
            profile.observations()
        );
    }

    /// D1's table: `MprGroupOutput::Append` -> ConfirmOnly, `MprGroupOutput::Overwrite` ->
    /// FailClosed (explicit task requirement, resolving D1's own "ConfirmOnly / FailClosed"
    /// ambiguity for `Overwrite` in favor of FailClosed).
    #[test]
    fn characterize_marks_append_confirm_only_and_overwrite_fail_closed() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <MorphologicalPhonologicalRuleFeatures>
            <MorphologicalPhonologicalRuleFeature id="mprA">A</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeature id="mprB">B</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="append" features="mprA"><Name>GAppend</Name></MorphologicalPhonologicalRuleFeatureGroup>
            <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="overwrite" features="mprB"><Name>GOverwrite</Name></MorphologicalPhonologicalRuleFeatureGroup>
          </MorphologicalPhonologicalRuleFeatures>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        assert_eq!(g.mpr_groups.len(), 2);

        let profile = characterize(&g);
        assert!(
            profile.observations().iter().any(|o| o.kind
                == CharacteristicKind::MprGroupAppend
                && o.disposition == Disposition::ConfirmOnly),
            "Append MPR group must characterize ConfirmOnly: {:?}",
            profile.observations()
        );
        assert!(
            profile.observations().iter().any(|o| o.kind
                == CharacteristicKind::MprGroupOverwrite
                && o.disposition == Disposition::FailClosed),
            "Overwrite MPR group must characterize FailClosed: {:?}",
            profile.observations()
        );
    }

    /// `fix-multitable-fst-compilation`: two tables with DISJOINT representations characterize
    /// `MultiTable`/`ConfigPredicate` (D1's table: `ConfirmOnly` unless/until a predicate proves
    /// `Admit`) — never `FailClosed` outright, since the threading fix makes per-rule resolution
    /// faithful.
    #[test]
    fn characterize_marks_disjoint_multi_table_config_predicate() {
        let g = load(TWO_TABLE_DISJOINT_XML);
        assert_eq!(g.char_tables.len(), 2);

        let profile = characterize(&g);
        assert!(
            profile.observations().iter().any(|o| o.kind
                == CharacteristicKind::MultiTable
                && o.disposition == Disposition::ConfigPredicate),
            "multi-table (disjoint) must characterize ConfigPredicate: {:?}",
            profile.observations()
        );
        let detail = profile
            .multi_table_detail()
            .expect("MultiTable must carry a MultiTableDetail");
        assert_eq!(detail.table_count, 2);
        assert!(detail.representations_pairwise_disjoint);
        assert!(detail.shared_representation_witness.is_none());
    }

    /// Positive witness (task 2.1): [`MultiTableFaithfulThreadingPredicate`] admits `ConfirmOnly`
    /// (never `Refuse`) for two tables with disjoint representations — the exact
    /// `two-table-symbol-divergence` shape `tests/two_table_symbol_divergence.rs` proves matches
    /// the oracle end to end.
    #[test]
    fn multi_table_predicate_confirm_only_when_tables_disjoint() {
        let g = load(TWO_TABLE_DISJOINT_XML);
        let profile = characterize(&g);
        let predicate = MultiTableFaithfulThreadingPredicate;
        // Node-agnostic (module doc) -- any PlanNodeKind works; reuse `leaf_for` for convenience.
        let verdict = predicate.evaluate(&profile, &leaf_for(PRuleId(0)));
        assert_eq!(
            verdict,
            PredicateVerdict::ConfirmOnly,
            "disjoint multi-table must be ConfirmOnly, not Refuse or Admit"
        );
    }

    /// Negative witness (task 2.1): two tables that SHARE a literal representation (the residual
    /// case the threading fix cannot make faithful, module doc) must `Refuse`, naming the shared
    /// representation.
    #[test]
    fn multi_table_predicate_refuses_when_tables_share_a_representation() {
        let g = load(TWO_TABLE_OVERLAPPING_XML);
        assert_eq!(g.char_tables.len(), 2);
        let profile = characterize(&g);
        let detail = profile
            .multi_table_detail()
            .expect("MultiTable must carry a MultiTableDetail");
        assert!(!detail.representations_pairwise_disjoint);
        assert!(detail
            .shared_representation_witness
            .as_deref()
            .unwrap_or_default()
            .contains("\"p\""));

        let predicate = MultiTableFaithfulThreadingPredicate;
        match predicate.evaluate(&profile, &leaf_for(PRuleId(0))) {
            PredicateVerdict::Refuse(diag) => {
                assert_eq!(diag.predicate, "multi-table.faithful-table-threading");
            }
            other => panic!("expected Refuse for overlapping-representation tables, got {other:?}"),
        }
    }

    /// A single-table grammar never observes `MultiTable` at all, and the predicate vacuously
    /// `Admit`s -- the byte-identical, never-buggy ordinary case.
    #[test]
    fn multi_table_predicate_admits_vacuously_for_single_table_grammar() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>SingleTable</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        assert_eq!(g.char_tables.len(), 1);
        let profile = characterize(&g);
        assert!(
            !profile
                .observations()
                .iter()
                .any(|o| o.kind == CharacteristicKind::MultiTable),
            "a single-table grammar must never observe MultiTable at all"
        );
        let predicate = MultiTableFaithfulThreadingPredicate;
        assert_eq!(
            predicate.evaluate(&profile, &leaf_for(PRuleId(0))),
            PredicateVerdict::Admit
        );
    }

    const TWO_TABLE_DISJOINT_XML: &str = r#"<HermitCrabInput><Language><Name>TwoTableDisjoint</Name>
      <CharacterDefinitionTable id="t0"><Name>T0</Name>
        <SegmentDefinitions><SegmentDefinition id="c0a"><Representations><Representation>p</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <CharacterDefinitionTable id="t1"><Name>T1</Name>
        <SegmentDefinitions><SegmentDefinition id="c1a"><Representations><Representation>k</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <Strata>
        <Stratum characterDefinitionTable="t0"><Name>S0</Name></Stratum>
        <Stratum characterDefinitionTable="t1"><Name>S1</Name></Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

    const TWO_TABLE_OVERLAPPING_XML: &str = r#"<HermitCrabInput><Language><Name>TwoTableOverlap</Name>
      <CharacterDefinitionTable id="t0"><Name>T0</Name>
        <SegmentDefinitions><SegmentDefinition id="c0a"><Representations><Representation>p</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <CharacterDefinitionTable id="t1"><Name>T1</Name>
        <SegmentDefinitions><SegmentDefinition id="c1a"><Representations><Representation>p</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <Strata>
        <Stratum characterDefinitionTable="t0"><Name>S0</Name></Stratum>
        <Stratum characterDefinitionTable="t1"><Name>S1</Name></Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

    /// An ordinary affix + iterative-rewrite grammar (no Compounding, no Unordered strata, no MPR
    /// groups, no Simultaneous/RightToLeft/Metathesis rules, no true reduplication, no dropped-LHS
    /// output) must characterize with NO FailClosed and NO ConfigPredicate observations at all.
    #[test]
    fn ordinary_affix_and_iterative_rewrite_grammar_characterizes_proven() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>Ordinary</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cb" /></SegmentNaturalClass></NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="pr1" morphologicalRules="mr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <MorphologicalRule id="mr1">
                  <Name>-a</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput>
                        <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence>
                      </MorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="stem" />
                        <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                      </MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </MorphologicalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = load(XML);

        let profile = characterize(&g);
        assert!(
            !profile.has_disposition(Disposition::FailClosed),
            "ordinary grammar must have NO FailClosed observations: {:?}",
            profile.observations()
        );
        assert!(
            !profile.has_disposition(Disposition::ConfigPredicate),
            "ordinary grammar must have NO ConfigPredicate observations either: {:?}",
            profile.observations()
        );
        // Sanity: it DOES characterize the expected Proven/ConfirmOnly-free constructs.
        assert!(profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::Affixation));
        assert!(profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::IterativeRewrite));
        assert!(profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::LeftToRightRewrite));
        assert!(profile
            .observations()
            .iter()
            .any(|o| o.kind == CharacteristicKind::OrderedMorphRuleApplication));
    }

    // ---------------------------------------------------------------------------------------
    // simultaneous.subrule-overlap (D3)
    // ---------------------------------------------------------------------------------------

    const SIMULTANEOUS_PROBE_XML: &str = r#"<HermitCrabInput><Language><Name>SimultaneousOverlapProbe</Name>
      <MorphologicalPhonologicalRuleFeatures>
        <MorphologicalPhonologicalRuleFeature id="mprA">Alpha</MorphologicalPhonologicalRuleFeature>
      </MorphologicalPhonologicalRuleFeatures>
      <PhonologicalFeatureSystem>
        <SymbolicFeature id="featCons"><Name>cons</Name>
          <Symbols><Symbol id="symConsP">+</Symbol><Symbol id="symConsM">-</Symbol></Symbols>
        </SymbolicFeature>
        <SymbolicFeature id="featVoi"><Name>voi</Name>
          <Symbols><Symbol id="symVoiP">+</Symbol><Symbol id="symVoiM">-</Symbol></Symbols>
        </SymbolicFeature>
      </PhonologicalFeatureSystem>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations>
            <FeatureValue feature="featCons" symbolValues="symConsP" />
            <FeatureValue feature="featVoi" symbolValues="symVoiM" />
          </SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses>
        <FeatureNaturalClass id="ncStop"><Name>Stop</Name>
          <FeatureValue feature="featCons" symbolValues="symConsP" />
        </FeatureNaturalClass>
        <FeatureNaturalClass id="ncVoiced"><Name>Voiced</Name>
          <FeatureValue feature="featVoi" symbolValues="symVoiP" />
        </FeatureNaturalClass>
        <FeatureNaturalClass id="ncVoiceless"><Name>Voiceless</Name>
          <FeatureValue feature="featVoi" symbolValues="symVoiM" />
        </FeatureNaturalClass>
      </NaturalClasses>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prAdmit" multipleApplicationOrder="simultaneous"><Name>admit</Name>
          <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule requiredMPRFeatures="mprA">
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
            <PhonologicalSubrule excludedMPRFeatures="mprA">
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prRefuseOverlap" multipleApplicationOrder="simultaneous"><Name>refuseOverlap</Name>
          <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prRefuseSelfOpaquing" multipleApplicationOrder="simultaneous"><Name>refuseSelfOpaquing</Name>
          <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
              <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncVoiceless" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
            </PhonologicalSubrule>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
    </Language></HermitCrabInput>"#;

    fn leaf_for(rule: PRuleId) -> PlanNodeKind {
        PlanNodeKind::Leaf {
            fragment: FragmentSpec::RewriteRule { rule },
            provenance: Provenance::RewriteRule(rule),
        }
    }

    /// Admit when the two subrules' MPR gates are provably disjoint (one requires what the other
    /// excludes) -- design.md D3's "cheap orthogonality early-out" for the common well-authored
    /// case, and neither subrule is self_opaquing.
    #[test]
    fn simultaneous_predicate_admits_mpr_disjoint_subrules() {
        let g = load(SIMULTANEOUS_PROBE_XML);
        let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
            panic!("expected rewrite rule at 0 (prAdmit)")
        };
        assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);

        let profile = characterize(&g);
        let predicate = SimultaneousSubruleOverlapPredicate;
        let verdict = predicate.evaluate(&profile, &leaf_for(PRuleId(0)));
        assert_eq!(
            verdict,
            PredicateVerdict::Admit,
            "mpr-disjoint, non-self-opaquing subrules must Admit"
        );
    }

    /// Refuse when neither subrule declares an MPR gate at all (overlap cannot be ruled out) and
    /// no `lower.rs` automaton intersection exists yet to prove non-overlap precisely -- the
    /// conservative fallback this step actually implements (see
    /// [`SimultaneousSubruleOverlapPredicate`]'s own doc).
    #[test]
    fn simultaneous_predicate_refuses_when_overlap_cannot_be_ruled_out() {
        let g = load(SIMULTANEOUS_PROBE_XML);
        let PhonRuleDef::Rewrite(r) = &g.prules[1] else {
            panic!("expected rewrite rule at 1 (prRefuseOverlap)")
        };
        assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);
        assert!(r.subrules[0].required_mpr.is_empty() && r.subrules[0].excluded_mpr.is_empty());

        let profile = characterize(&g);
        let predicate = SimultaneousSubruleOverlapPredicate;
        let verdict = predicate.evaluate(&profile, &leaf_for(PRuleId(1)));
        match verdict {
            PredicateVerdict::Refuse(diag) => {
                assert_eq!(diag.predicate, "simultaneous.subrule-overlap");
            }
            other => panic!("expected Refuse, got {other:?}"),
        }
    }

    /// Refuse when a subrule is self_opaquing -- D3: "do not attempt Admit" regardless of mpr
    /// gating.
    #[test]
    fn simultaneous_predicate_refuses_self_opaquing_subrule() {
        let g = load(SIMULTANEOUS_PROBE_XML);
        let PhonRuleDef::Rewrite(r) = &g.prules[2] else {
            panic!("expected rewrite rule at 2 (prRefuseSelfOpaquing)")
        };
        assert!(
            r.subrules[0].self_opaquing,
            "prRefuseSelfOpaquing's first subrule must be self_opaquing (RHS Voiced vs \
             RightEnvironment Voiceless, disjoint voi bits)"
        );

        let profile = characterize(&g);
        let predicate = SimultaneousSubruleOverlapPredicate;
        let verdict = predicate.evaluate(&profile, &leaf_for(PRuleId(2)));
        match verdict {
            PredicateVerdict::Refuse(diag) => {
                assert!(diag.witness.contains("self_opaquing"));
            }
            other => panic!("expected Refuse, got {other:?}"),
        }
    }

    /// A non-Simultaneous (Iterative) rule is always Admit -- D3's own first line.
    #[test]
    fn simultaneous_predicate_admits_iterative_rule_unconditionally() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        let profile = characterize(&g);
        let predicate = SimultaneousSubruleOverlapPredicate;
        assert_eq!(
            predicate.evaluate(&profile, &leaf_for(PRuleId(0))),
            PredicateVerdict::Admit
        );
    }

    // ---------------------------------------------------------------------------------------
    // Stage 1B (`lower-fst-pattern-environments`): the real automaton intersection replacing the
    // conservative unconditional-Refuse fallback. No `PhonologicalFeatureSystem` in any of these
    // three fixtures -- deliberately: every `Context` node's self-opaquing pin-bit computation
    // (`pg_grammar::load::pattern_node_pin_bits`) is vacuously empty with zero declared features,
    // so `self_opaquing` is `false` for every subrule below regardless of environment shape,
    // isolating exactly the NEW code path (the survives-both-early-outs branch) these tests target
    // -- the SAME reason `SIMULTANEOUS_PROBE_XML`'s own `prAdmit`/`prRefuseOverlap` rules avoid
    // self-opaquing by declaring no `Environment` at all; these need a real environment, so they
    // avoid it via "no features to mismatch on" instead. Also no MPR features declared, so
    // `mpr_gates_disjoint` is `false` (two empty `MprSet`s never overlap) for every pair -- the
    // mpr-gate early-out never fires either, so every case here is decided purely by the NEW
    // lowered-span intersection.
    // ---------------------------------------------------------------------------------------

    /// Two subrules whose RIGHT environments are mutually exclusive `SegmentNaturalClass`es
    /// (`Front` = {`i`}, `Back` = {`u`}, no shared segment) -- genuinely CANNOT overlap at the
    /// shared focus position. Neither is mpr-disjoint nor self-opaquing, so the PRIOR
    /// unconditional-`Refuse` fallback would have rounded this pair to `Refuse`; the real
    /// automaton intersection (Stage 1B) now proves their spans disjoint and `Admit`s -- the
    /// whole point of this step (strictly fewer refusals, never more, per ADR 0001).
    #[test]
    fn simultaneous_predicate_admits_genuinely_non_overlapping_subrules_via_lowered_span() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>SimLowerAdmit</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cStop"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cFront"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cBack"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncStop"><Name>Stop</Name><Segment segment="cStop" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncFront"><Name>Front</Name><Segment segment="cFront" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncBack"><Name>Back</Name><Segment segment="cBack" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1" multipleApplicationOrder="simultaneous"><Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncFront" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
            panic!("expected rewrite rule at 0")
        };
        assert_eq!(r.mode, RewriteMode::Simultaneous);
        assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);
        // No MPR features declared anywhere in this fixture -- both subrules' gates are empty, so
        // `mpr_gates_disjoint` cannot possibly short-circuit this pair (two empty `MprSet`s never
        // "overlap"); the outcome below is decided purely by the new lowered-span intersection.
        assert!(r.subrules[0].required_mpr.is_empty() && r.subrules[0].excluded_mpr.is_empty());
        assert!(r.subrules[1].required_mpr.is_empty() && r.subrules[1].excluded_mpr.is_empty());

        let profile = characterize(&g);
        let predicate = SimultaneousSubruleOverlapPredicate;
        assert_eq!(
            predicate.evaluate(&profile, &leaf_for(PRuleId(0))),
            PredicateVerdict::Admit,
            "Front/Back-flanked, non-mpr-disjoint, non-self-opaquing subrules must now Admit \
             via the real lowered-span intersection (previously Refuse under the conservative \
             fallback)"
        );
    }

    /// Two subrules whose RIGHT environments genuinely OVERLAP (one accepts `{Front, Back}`, the
    /// other accepts `{Back}` alone -- a shared member, not an identical automaton) must still
    /// `Refuse`, with a witness naming the real intersection.
    #[test]
    fn simultaneous_predicate_refuses_genuinely_overlapping_subrules_via_lowered_span() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>SimLowerRefuse</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cStop"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cFront"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cBack"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncStop"><Name>Stop</Name><Segment segment="cStop" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncFrontOrBack"><Name>FrontOrBack</Name><Segment segment="cFront" /><Segment segment="cBack" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncBack"><Name>Back</Name><Segment segment="cBack" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1" multipleApplicationOrder="simultaneous"><Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncFrontOrBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
            panic!("expected rewrite rule at 0")
        };
        assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);

        let profile = characterize(&g);
        let predicate = SimultaneousSubruleOverlapPredicate;
        match predicate.evaluate(&profile, &leaf_for(PRuleId(0))) {
            PredicateVerdict::Refuse(diag) => {
                assert_eq!(diag.predicate, "simultaneous.subrule-overlap");
                assert!(
                    diag.witness.contains("genuinely intersect"),
                    "witness should name the real intersection, not the old conservative \
                     wording: {diag:?}"
                );
            }
            other => panic!("expected Refuse (genuine overlap via shared Back member), got {other:?}"),
        }
    }

    /// A subrule whose right environment uses a `PatternNode::Anchor` (word-boundary condition) --
    /// a node kind [`crate::lower::lower_span`] does not represent -- must still conservatively
    /// `Refuse`, naming the unhandled kind, rather than silently `Admit` an unproven pair.
    #[test]
    fn simultaneous_predicate_refuses_unsupported_pattern_node_conservatively() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>SimLowerUnsupported</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="cStop"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cFront"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cBack"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncStop"><Name>Stop</Name><Segment segment="cStop" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncFront"><Name>Front</Name><Segment segment="cFront" /></SegmentNaturalClass>
            <SegmentNaturalClass id="ncBack"><Name>Back</Name><Segment segment="cBack" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1" multipleApplicationOrder="simultaneous"><Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate finalBoundaryCondition="true"><PhoneticSequence><SimpleContext naturalClass="ncFront" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
                  <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
            panic!("expected rewrite rule at 0")
        };
        assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);
        assert!(
            matches!(
                r.subrules[0].right_env.as_ref().unwrap().nodes.last(),
                Some(pg_grammar::model::PatternNode::Anchor(
                    pg_grammar::model::AnchorSide::Right
                ))
            ),
            "fixture must actually carry an Anchor node in subrule 0's right_env: {:?}",
            r.subrules[0].right_env
        );

        let profile = characterize(&g);
        let predicate = SimultaneousSubruleOverlapPredicate;
        match predicate.evaluate(&profile, &leaf_for(PRuleId(0))) {
            PredicateVerdict::Refuse(diag) => {
                assert_eq!(diag.predicate, "simultaneous.subrule-overlap");
                assert!(
                    diag.witness.contains("Anchor"),
                    "witness must name the unhandled Anchor node kind: {diag:?}"
                );
            }
            other => panic!("expected conservative Refuse naming Anchor, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------------------------------
    // Registry coverage
    // ---------------------------------------------------------------------------------------

    /// design.md D2 / spec.md: every `FailClosed`/`ConfigPredicate` characteristic must be
    /// discharged by >= 1 registered predicate, else the build "breaks" (here: the test fails,
    /// standing in for the CI-level enforcement a later step would wire up).
    #[test]
    fn default_registry_discharges_every_fail_closed_or_config_predicate_kind() {
        let registry = default_registry();
        let missing = undischarged_kinds(&registry);
        assert!(
            missing.is_empty(),
            "undischarged FailClosed/ConfigPredicate characteristics: {missing:?}"
        );
    }

    /// Every [`CharacteristicKind`] variant has an explicit (non-panicking) default disposition --
    /// re-derives the exhaustive match's own totality as an executable check, and doubles as a
    /// canary that [`CharacteristicKind::ALL`] hasn't drifted out of sync with the enum (a variant
    /// missing from `ALL` would simply not appear in this loop -- see `ALL`'s own doc for that
    /// documented gap).
    #[test]
    fn all_kinds_have_a_default_disposition() {
        for kind in CharacteristicKind::ALL {
            let _ = kind.default_disposition();
        }
        assert_eq!(
            CharacteristicKind::ALL.len(),
            19,
            "bumped from 18 by fix-multitable-fst-compilation's new CharacteristicKind::MultiTable"
        );
    }

    // ---------------------------------------------------------------------------------------
    // compose_envelope (D4, Step 2): meet lattice unit checks
    // ---------------------------------------------------------------------------------------

    fn diag(predicate: PredicateId, construct: &str) -> CapabilityDiagnostic {
        CapabilityDiagnostic {
            predicate,
            construct: construct.to_string(),
            witness: "unit-test witness".to_string(),
        }
    }

    /// D4's lattice, spelled out directly on [`meet`] (not via a whole grammar): `Refuse`
    /// dominates `ConfirmOnly` dominates `Admit`, in every pairing.
    #[test]
    fn meet_lattice_lines_up_with_d4() {
        assert_eq!(
            meet(CompileDecision::Admit, CompileDecision::Admit),
            CompileDecision::Admit
        );
        assert_eq!(
            meet(CompileDecision::Admit, CompileDecision::ConfirmOnly),
            CompileDecision::ConfirmOnly
        );
        assert_eq!(
            meet(CompileDecision::ConfirmOnly, CompileDecision::Admit),
            CompileDecision::ConfirmOnly
        );
        assert_eq!(
            meet(CompileDecision::ConfirmOnly, CompileDecision::ConfirmOnly),
            CompileDecision::ConfirmOnly
        );
        let d1 = diag("p1", "c1");
        assert_eq!(
            meet(CompileDecision::Admit, CompileDecision::Refuse(vec![d1.clone()])),
            CompileDecision::Refuse(vec![d1.clone()])
        );
        assert_eq!(
            meet(CompileDecision::Refuse(vec![d1.clone()]), CompileDecision::ConfirmOnly),
            CompileDecision::Refuse(vec![d1.clone()])
        );
    }

    /// Two `Refuse`s meet to carry BOTH diagnostics (union), not just one side's -- the "a caller
    /// sees every problem" requirement.
    #[test]
    fn meet_of_two_refuses_unions_diagnostics() {
        let d1 = diag("p1", "c1");
        let d2 = diag("p2", "c2");
        let merged = meet(
            CompileDecision::Refuse(vec![d1.clone()]),
            CompileDecision::Refuse(vec![d2.clone()]),
        );
        assert_eq!(merged, CompileDecision::Refuse(vec![d1, d2]));
    }

    /// Meeting a `Refuse` with an EQUAL diagnostic (the same construct reached via two DAG paths
    /// to a shared node) does not duplicate it.
    #[test]
    fn meet_of_two_refuses_deduplicates_identical_diagnostics() {
        let d1 = diag("p1", "c1");
        let merged = meet(
            CompileDecision::Refuse(vec![d1.clone()]),
            CompileDecision::Refuse(vec![d1.clone()]),
        );
        assert_eq!(merged, CompileDecision::Refuse(vec![d1]));
    }

    // ---------------------------------------------------------------------------------------
    // compose_envelope (D4, Step 2): end-to-end over characterize() + enumerate_default()
    // ---------------------------------------------------------------------------------------

    /// An ordinary affix + iterative-rewrite grammar (no Compounding, no Unordered strata, no MPR
    /// groups, no Simultaneous/RightToLeft/Metathesis rules, no true reduplication/circumfix) must
    /// compose to `Admit` -- the common well-authored case.
    #[test]
    fn compose_envelope_admits_ordinary_affix_and_iterative_rewrite_grammar() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>Ordinary</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cb" /></SegmentNaturalClass></NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="pr1" morphologicalRules="mr1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <MorphologicalRule id="mr1">
                  <Name>-a</Name>
                  <MorphologicalSubrules>
                    <MorphologicalSubrule id="sub1">
                      <MorphologicalInput>
                        <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence>
                      </MorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="stem" />
                        <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                      </MorphologicalOutput>
                    </MorphologicalSubrule>
                  </MorphologicalSubrules>
                </MorphologicalRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        let plan = enumerated_plan(&g);
        let registry = default_registry();

        assert_eq!(compose_envelope(&g, &plan, &registry), CompileDecision::Admit);
    }

    /// A grammar with a `Compounding` rule must compose to `Refuse`, with a diagnostic naming
    /// compounding.
    #[test]
    fn compose_envelope_refuses_compounding_grammar() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        let plan = enumerated_plan(&g);
        let registry = default_registry();

        match compose_envelope(&g, &plan, &registry) {
            CompileDecision::Refuse(diags) => {
                assert!(
                    diags.iter().any(|d| d.construct.contains("Compounding")),
                    "expected a diagnostic naming Compounding: {diags:?}"
                );
            }
            other => panic!("expected Refuse, got {other:?}"),
        }
    }

    /// A grammar with `MorphRuleOrder::Unordered` must compose to `Refuse`.
    #[test]
    fn compose_envelope_refuses_unordered_morph_rule_order_grammar() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <Strata>
            <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
              <Name>S</Name>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        let plan = enumerated_plan(&g);
        let registry = default_registry();

        match compose_envelope(&g, &plan, &registry) {
            CompileDecision::Refuse(diags) => {
                assert!(
                    diags
                        .iter()
                        .any(|d| d.construct.contains("UnorderedMorphRuleApplication")),
                    "expected a diagnostic naming UnorderedMorphRuleApplication: {diags:?}"
                );
            }
            other => panic!("expected Refuse, got {other:?}"),
        }
    }

    /// A grammar with an `MprGroupOutput::Append` group and nothing worse must compose to
    /// `ConfirmOnly` -- not `Admit` (the group is real, unproven-to-Admit material) and not
    /// `Refuse` (nothing FailClosed is present).
    #[test]
    fn compose_envelope_confirm_only_for_append_group_alone() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>AppendOnly</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <MorphologicalPhonologicalRuleFeatures>
            <MorphologicalPhonologicalRuleFeature id="mprA">A</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="append" features="mprA"><Name>GAppend</Name></MorphologicalPhonologicalRuleFeatureGroup>
          </MorphologicalPhonologicalRuleFeatures>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <LexicalEntries>
                <LexicalEntry id="e1" partOfSpeech="posV">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>e1</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        assert!(
            !g.mpr_groups.is_empty(),
            "fixture must declare an MprGroup at all"
        );
        let plan = enumerated_plan(&g);
        let registry = default_registry();

        assert_eq!(
            compose_envelope(&g, &plan, &registry),
            CompileDecision::ConfirmOnly
        );
    }

    /// A grammar with a `Simultaneous` rewrite rule whose subrules are provably mpr-disjoint (and
    /// not self-opaquing) must compose to `Admit`.
    #[test]
    fn compose_envelope_admits_simultaneous_rule_with_mpr_disjoint_subrules() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>SimAdmit</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <MorphologicalPhonologicalRuleFeatures>
            <MorphologicalPhonologicalRuleFeature id="mprA">Alpha</MorphologicalPhonologicalRuleFeature>
          </MorphologicalPhonologicalRuleFeatures>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="c2"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="c1" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1" multipleApplicationOrder="simultaneous">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule requiredMPRFeatures="mprA">
                  <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
                <PhonologicalSubrule excludedMPRFeatures="mprA">
                  <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="pr1">
              <Name>S</Name>
              <LexicalEntries>
                <LexicalEntry id="e1" partOfSpeech="posV">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>e1</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
            panic!("expected rewrite rule at 0")
        };
        assert_eq!(r.mode, RewriteMode::Simultaneous);
        assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);

        let plan = enumerated_plan(&g);
        let registry = default_registry();

        assert_eq!(compose_envelope(&g, &plan, &registry), CompileDecision::Admit);
    }

    /// The same shape, but neither subrule declares an MPR gate -- overlap can't be ruled out, so
    /// this must compose to `Refuse`.
    #[test]
    fn compose_envelope_refuses_simultaneous_rule_when_overlap_cannot_be_ruled_out() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>SimRefuse</Name>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="c2"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses>
            <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="c1" /></SegmentNaturalClass>
          </NaturalClasses>
          <PhonologicalRuleDefinitions>
            <PhonologicalRule id="pr1" multipleApplicationOrder="simultaneous">
              <Name>PR</Name>
              <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
              <PhonologicalSubrules>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
                <PhonologicalSubrule>
                  <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
                </PhonologicalSubrule>
              </PhonologicalSubrules>
            </PhonologicalRule>
          </PhonologicalRuleDefinitions>
          <Strata>
            <Stratum characterDefinitionTable="t1" phonologicalRules="pr1">
              <Name>S</Name>
              <LexicalEntries>
                <LexicalEntry id="e1">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        let PhonRuleDef::Rewrite(r) = &g.prules[0] else {
            panic!("expected rewrite rule at 0")
        };
        assert!(!r.subrules[0].self_opaquing && !r.subrules[1].self_opaquing);
        assert!(r.subrules[0].required_mpr.is_empty() && r.subrules[0].excluded_mpr.is_empty());

        let plan = enumerated_plan(&g);
        let registry = default_registry();

        match compose_envelope(&g, &plan, &registry) {
            CompileDecision::Refuse(diags) => {
                assert!(
                    diags.iter().any(|d| d.predicate == "simultaneous.subrule-overlap"),
                    "expected a simultaneous.subrule-overlap diagnostic: {diags:?}"
                );
            }
            other => panic!("expected Refuse, got {other:?}"),
        }
    }

    /// Meet correctness: a grammar that is BOTH ConfirmOnly-worthy (an Append group) AND
    /// Refuse-worthy (a Compounding rule) must compose to `Refuse` overall (Refuse dominates), and
    /// the `Refuse` must carry a diagnostic for the refusing construct (Compounding), not just
    /// silently drop it because a milder ConfirmOnly construct is ALSO present.
    #[test]
    fn compose_envelope_meet_correctness_refuse_dominates_confirm_only() {
        const XML: &str = r#"<HermitCrabInput><Language><Name>AppendPlusCompound</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
          <MorphologicalPhonologicalRuleFeatures>
            <MorphologicalPhonologicalRuleFeature id="mprA">A</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="append" features="mprA"><Name>GAppend</Name></MorphologicalPhonologicalRuleFeatureGroup>
          </MorphologicalPhonologicalRuleFeatures>
          <CharacterDefinitionTable id="t1"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="c1" /></SegmentNaturalClass></NaturalClasses>
          <Strata>
            <Stratum characterDefinitionTable="t1">
              <Name>S</Name>
              <MorphologicalRuleDefinitions>
                <CompoundingRule id="cr1">
                  <Name>Compound</Name>
                  <CompoundingSubrules>
                    <CompoundingSubrule>
                      <HeadMorphologicalInput>
                        <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </HeadMorphologicalInput>
                      <NonHeadMorphologicalInput>
                        <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                      </NonHeadMorphologicalInput>
                      <MorphologicalOutput>
                        <CopyFromInput index="n0" />
                        <CopyFromInput index="h0" />
                      </MorphologicalOutput>
                    </CompoundingSubrule>
                  </CompoundingSubrules>
                </CompoundingRule>
              </MorphologicalRuleDefinitions>
              <LexicalEntries>
                <LexicalEntry id="e1" partOfSpeech="posV">
                  <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
                  <Gloss>e1</Gloss>
                </LexicalEntry>
              </LexicalEntries>
            </Stratum>
          </Strata>
        </Language></HermitCrabInput>"#;
        let g = load(XML);
        assert!(!g.mpr_groups.is_empty(), "fixture must declare an MprGroup");
        assert!(
            g.mrules.iter().any(|m| matches!(m, MorphRuleDef::Compounding(_))),
            "fixture must declare a Compounding rule"
        );

        let plan = enumerated_plan(&g);
        let registry = default_registry();

        match compose_envelope(&g, &plan, &registry) {
            CompileDecision::Refuse(diags) => {
                assert!(
                    diags.iter().any(|d| d.construct.contains("Compounding")),
                    "Refuse must carry a diagnostic naming Compounding, not just meet away to a \
                     bare ConfirmOnly: {diags:?}"
                );
            }
            other => panic!(
                "expected Refuse (Refuse dominates ConfirmOnly per D4), got {other:?}"
            ),
        }
    }
}
