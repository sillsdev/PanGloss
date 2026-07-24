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
//! design.md D3 works through in full — see that type's own doc for the conservative fallback this
//! step takes in place of the `lower-fst-pattern-environments` (Stage 1B) automaton intersection,
//! which does not exist in this crate yet (confirmed: no `lower.rs`/pattern-to-`Fsm` facility
//! anywhere in `pg-fst`/`pg-foma` as of this step — grep the workspace before assuming otherwise if
//! this doc goes stale).
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
//! yet (it needs `lower-fst-pattern-environments`, same prerequisite [`SimultaneousSubruleOverlapPredicate`]
//! is blocked on), so a `Union`/`Compose` node's "own predicate verdicts" are simply empty today —
//! flagged as a documented gap, not a silent omission; see [`compose_envelope`]'s own doc for the
//! per-construct plan-node-mapping judgment calls.

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

/// Per-subrule gate/opacity facts a [`RewriteRuleDef`](pg_grammar::model::RewriteRuleDef)'s
/// [`ObservationDetail::SimultaneousRewrite`] carries — exactly what
/// [`SimultaneousSubruleOverlapPredicate`] (D3) needs, without re-walking the `Grammar` at
/// evaluate-time (the profile is meant to be a self-contained projection, design.md D1).
#[derive(Debug, Clone, Copy)]
pub struct SubruleGateInfo {
    pub index: usize,
    pub required_mpr: MprSet,
    pub excluded_mpr: MprSet,
    pub self_opaquing: bool,
}

/// [`ObservationDetail::SimultaneousRewrite`]'s payload: one rule's full subrule-gate table.
#[derive(Debug, Clone)]
pub struct SimultaneousRewriteDetail {
    pub rule: PRuleId,
    pub subrules: Vec<SubruleGateInfo>,
}

/// Extra structured data an observation needs beyond `kind`/`disposition`/`location`, for the
/// characteristics that a predicate must inspect at finer grain than "did this occur at all"
/// (design.md D2/D3). Most characteristics carry `None` — only [`CharacteristicKind::
/// SimultaneousRewrite`] needs [`Self::SimultaneousRewrite`] today (D3's worked example).
#[derive(Debug, Clone)]
pub enum ObservationDetail {
    None,
    SimultaneousRewrite(SimultaneousRewriteDetail),
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
/// # The conservative fallback this step actually takes (no `lower.rs` yet)
/// D3's precise test is `intersect(span(s_i), span(s_j))` where `span(s) = left_env · lhs_focus ·
/// right_env`, lowered to an `Fsm` via `lower-fst-pattern-environments` (Stage 1B). That facility
/// does **not exist yet** in this workspace — confirmed by grep: no `lower.rs`/pattern-to-`Fsm`
/// module anywhere under `pg-fst`/`pg-foma` as of this step (`crate::plan`'s own leaf shapes carry
/// no `Fsm` either — see that module's own doc, "no live `Fsm` here — that is Step 2"). So instead
/// of the true automaton intersection, every pair that survives the `mpr_gates_disjoint` early-out
/// (i.e. whose overlap is NOT ruled out by MPR gating alone) is rounded straight to `Refuse` —
/// D3's own words: "any approximation rounds toward 'overlap possible' (Refuse)." **TODO**: once
/// `lower-fst-pattern-environments` lands, replace that unconditional `Refuse` with the real
/// `intersect(span(s_i), span(s_j))` test, which will `Admit` strictly more pairs than today
/// (never fewer — over-refusal only ever narrows as proof machinery improves, per ADR 0001).
///
/// # Provenance
/// [`EvidenceProvenance::Structural`]: every fact this predicate's `evaluate` reads
/// (`self_opaquing`, `required_mpr`, `excluded_mpr`) is a directly-inspectable `model.rs` field —
/// no foma black-box oracle call is made at this step. This is a judgment call flagged for review:
/// design.md D3 reserves `Structural` for "the controllable composition path (we intersect real
/// lowered automata)," which this step does not yet do; `Behavioral` (reserved for "the black-box
/// foma path... automata unobservable") does not fit either, since no foma call happens here at
/// all. `Structural` was chosen because the EVIDENCE KIND (direct field reads) matches that
/// definition even though the PROOF (automaton intersection) is not yet built; once
/// `lower-fst-pattern-environments` lands the provenance value should not need to change, only the
/// `Refuse`-fallback TODO above does.
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

                // Conservative fallback (see this type's own doc): no `lower-fst-pattern-
                // environments` automaton intersection exists yet, so any pair not proven
                // mpr-disjoint rounds to Refuse rather than risk an unproven Admit.
                return PredicateVerdict::Refuse(CapabilityDiagnostic {
                    predicate: self.id(),
                    construct: format!("prule {} subrules {}/{}", rule.0, a.index, b.index),
                    witness: format!(
                        "subrules {} and {} are not proven mpr-gate-disjoint, and \
                         lower-fst-pattern-environments (Stage 1B) is not yet available to \
                         intersect their lowered environment automata; conservatively rounding \
                         toward overlap-possible (TODO: tighten once lower.rs lands)",
                        a.index, b.index
                    ),
                });
            }
        }

        PredicateVerdict::Admit
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

/// The minimal registry this step ships: the one REAL predicate
/// ([`SimultaneousSubruleOverlapPredicate`]), plus an explicit [`FailClosedPlaceholder`] for every
/// other `FailClosed`/`ConfigPredicate` characteristic — proving the coverage contract holds today
/// without pretending any of those other constructs has a real proof yet.
pub fn default_registry() -> PredicateRegistry {
    let mut r = PredicateRegistry::new();
    r.register(Box::new(SimultaneousSubruleOverlapPredicate));
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
        assert_eq!(CharacteristicKind::ALL.len(), 18);
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
