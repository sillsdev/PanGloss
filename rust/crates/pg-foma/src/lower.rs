//! Stage 1B (`openspec/changes/lower-fst-pattern-environments`): the shared pattern/environment →
//! FST lowering seam `openspec/changes/add-capability-characteristics-check/design.md` D3 needs
//! for `simultaneous.subrule-overlap`'s REAL automaton-intersection test, replacing
//! `capability.rs`'s prior conservative unconditional-`Refuse` fallback — see
//! [`crate::capability::SimultaneousSubruleOverlapPredicate`]'s own doc for exactly what this
//! replaces and how.
//!
//! # Scope of the ORIGINAL step (D3's own worked predicate)
//! `lower-fst-pattern-environments`'s own `design.md` asks for one lowering seam covering anchors,
//! polarity, groups, alternation, table identity, and quantifier metadata, and migrating EVERY
//! existing replacement caller (`replace.rs`/`gate.rs`) onto it (`tasks.md` 1.1-1.2, 2.1-2.3). The
//! original step was narrower, scoped to exactly what D3's worked predicate needs: [`lower_span`]
//! lowers one subrule's `left_env · lhs_focus · right_env` triple (D3's own `span(s)` formula) into
//! foma acceptors, and [`spans_overlap`] tests two such spans for a non-empty intersection at the
//! shared focus position. At that point `replace.rs`/`gate.rs` were UNTOUCHED beyond three
//! visibility bumps (`pattern_slots`/`resolve_alpha_tuples`/`render_slots` going `pub(crate)`) so
//! this module could REUSE their pattern semantics rather than re-derive it; migrating
//! `replace.rs`'s OWN rewrite-rule compilation onto this seam (design.md's "Migrate existing
//! replacement callers", `tasks.md` 2.1) was flagged as a separate, larger follow-on not attempted
//! in that step. **That follow-on is THIS step** — see "What is reused vs. newly written vs. MOVED
//! HERE" below for what changed. Full Stage 1B coverage (multi-table ownership, alternation) is
//! still future work — see [`UnsupportedPatternNode`]'s own doc for exactly which node kinds
//! [`lower_span`] does and does not represent. Quantifier metadata is PARTIALLY covered,
//! transparently: `openspec/changes/compile-bounded-fst-quantifiers` teaches [`pattern_slots`]
//! itself to accept a finitely bounded, alpha-free `PatternNode::Quantifier` (a new
//! `Slot::Repeat`, that variant's own doc), and `openspec/changes/build-unbounded-quantifier-support`
//! widens that SAME acceptance to a genuinely UNBOUNDED, alpha-free `Quantifier` too (`max: None`,
//! `Slot::Repeat`'s own doc for the native `E*`/`E^>N` construction) — since [`lower_span`] calls
//! `pattern_slots` directly (never re-deriving pattern coverage), either shape anywhere in
//! `left_env`/`focus`/`right_env` lowers for free, no code change needed for that. An inverted-bound
//! (`min > max`, `max` concrete), over-budget-finite (`max` concrete but past
//! [`MAX_QUANTIFIER_BOUND`]), or alpha-nested quantifier is UNCHANGED: `pattern_slots` still returns
//! `None` for it, and [`UnsupportedPatternNode::Quantifier`] is still the typed reason
//! [`diagnose_unsupported`] reports.
//!
//! # What is reused vs. newly written vs. MOVED HERE (migration follow-on, this step)
//! This step is the "migrate `replace.rs`'s own rewrite-rule pattern compilation onto the shared
//! seam" follow-on the section above flags as NOT attempted in the original step — `tasks.md`
//! 2.1's "adapt existing replacement callers without changing their network semantics". The
//! dependency used to run backwards for a true seam (this module BORROWING `replace.rs`'s
//! pattern-slot/alpha-tuple/rendering logic via three visibility bumps); it is now INVERTED:
//! [`Slot`], [`pattern_slots`], [`slots_from_nodes`], [`resolve_alpha_tuples`], [`render_slots`],
//! [`AlphaAssignment`], [`TupleReport`], `class_members`, `slots_contain_alpha`, and
//! `MAX_QUANTIFIER_BOUND` (all formerly defined in `replace.rs`, `pub(crate)`-reused from there)
//! now live HERE as this module's own canonical pattern-lowering vocabulary — moved byte-for-byte
//! (logic untouched, only doc cross-references and a handful of `crate::replace` path prefixes
//! updated to plain in-module names), not re-derived. `replace.rs` no longer defines any of them;
//! it re-exports every one at its OLD path (`pub(crate) use crate::lower::{Slot, pattern_slots,
//! resolve_alpha_tuples, render_slots};` / `pub use crate::lower::{AlphaAssignment, TupleReport};`)
//! so every existing caller keeps compiling completely unmodified — `replace.rs`'s own
//! rewrite-rule/metathesis compilation (`compile_rewrite_rule_subset`, `compile_metathesis_rule`,
//! `compile_rtl_branch_net`, `slot_candidates`), `capability.rs`'s structural probes
//! (`crate::replace::pattern_slots`/`crate::replace::Slot::Alpha`/`crate::replace::Slot::Repeat`,
//! untouched — that file is a concurrent agent's exclusive territory this step does not open),
//! every `tests/phase_c_*` gate, and every `pg_foma::replace::TupleReport`-importing example all
//! resolve through the re-export, unchanged.
//!
//! Two pieces deliberately did NOT move, kept in `replace.rs` on purpose:
//! - [`crate::replace::SegAlphabet`] (the char-def ↔ PUA-token codec, still `pub`, untouched) —
//!   general token-alphabet infrastructure `emit.rs`/`uflexc.rs`/`gate.rs`/`enumerate.rs`/
//!   `oracle.rs`/`capability.rs`/`capability_entry.rs`/`plan_interaction_coverage.rs` all depend
//!   on directly, not something this change's own scope (pattern/environment → FST lowering)
//!   claims ownership of. Still imported here (below) exactly as before this step.
//! - [`crate::replace::owning_table`]/[`crate::replace::owning_table_for_metathesis`] (rule →
//!   owning-stratum → `CharDefTable` resolution) — rule/stratum bookkeeping, not pattern lowering;
//!   [`lower_span`]'s own callers (`capability.rs`) already resolve the table themselves before
//!   calling in (this function's own doc, "the caller's own contract"), so this module never
//!   needed to call either function itself. Left in `replace.rs` as the more defensible home.
//!
//! Newly written in the ORIGINAL step, unchanged by this one: [`lower_span`] itself (how to
//! COMBINE the pattern-lowering vocabulary above into acceptors — the `Σ*`-padding construction
//! its own doc works through), [`UnsupportedPatternNode`] (the typed disposition evidence
//! design.md's `spec.md` asks for — "a typed unsupported disposition... does not omit or weaken
//! the node"), and [`spans_overlap`] (the intersect-nonempty test over
//! `foma::constructions::{fsm_intersect, fsm_union, fsm_concat, fsm_universal}` /
//! `foma::structures::fsm_isempty`).

use std::collections::HashSet;

use foma::constructions::{fsm_concat, fsm_intersect, fsm_union, fsm_universal};
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::structures::{fsm_empty_set, fsm_empty_string, fsm_isempty};
use foma::types::Fsm;

use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::{AnchorSide, Grammar, NaturalClassKind, Pattern, PatternNode, VarId};

use crate::replace::SegAlphabet;

// =================================================================================================
// Natural-class member resolution (exact, from the model's own `NaturalClassKind` — never
// re-derived through a matcher-oriented helper whose semantics are tuned for a different job).
//
// MOVED HERE from `replace.rs` (`lower-fst-pattern-environments` Stage 1B migration follow-on,
// module top doc) -- logic byte-for-byte unchanged, `replace.rs` re-exports the still-`pub(crate)`
// names below it needs (`Slot`, `pattern_slots`) at their old paths.
// =================================================================================================

/// One class's members, resolved from [`NaturalClassKind`] with a given set of alpha-bound
/// feature lanes excluded from the `Feature`-kind pin test (module doc: an alpha-bound feature is
/// NOT a fixed pin — its value is resolved per tuple, see [`resolve_alpha_tuples`]).
fn class_members(
    g: &Grammar,
    table: &CharDefTable,
    nat_class: pg_grammar::model::NatClassId,
    exclude_lanes: &HashSet<usize>,
) -> Vec<CharDefId> {
    match &g.natural_classes[nat_class.0 as usize].kind {
        // Explicit segment list: verbatim, exact (module doc — never re-derived via a feature
        // reconstruction that could silently diverge from the authored list).
        NaturalClassKind::Segments(ids) => ids.clone(),
        NaturalClassKind::Feature(pairs) => table
            .iter()
            .filter(|(_, cd)| cd.kind() == CharDefKind::Segment)
            .filter(|(_, cd)| {
                pairs.iter().all(|(f, bits)| {
                    exclude_lanes.contains(&(f.0 as usize))
                        || (cd.feature_lanes()[f.0 as usize] & bits.0 != 0)
                })
            })
            .map(|(id, _)| id)
            .collect(),
    }
}

// =================================================================================================
// Pattern -> slot list (one slot per PatternNode, in document order); `None` on any construct
// this prototype doesn't render (Quantifier/Anchor/CharDef-of-unknown-kind never seen here).
// =================================================================================================

/// Which additional pattern-node shapes a particular [`pattern_slots`]/[`slots_from_nodes`] CALLER
/// may accept, beyond the floor every caller has always shared (`Context`/`CharDef`/a well-formed
/// `Quantifier`). `pattern_slots` is a single shared lowering seam deliberately reused by THREE
/// independent consumers with DIFFERENT verification obligations (module top doc's own "reuse, not
/// re-derive" discipline: [`lower_span`] for `crate::capability::SimultaneousSubruleOverlapPredicate`
/// (D3's `hc.dll`-oracle-verified span-intersection test), `crate::replace::compile_rewrite_rule_
/// subset`/`crate::replace::compile_metathesis_rule` for the real rewrite-rule/metathesis compile) --
/// widening what ONE consumer accepts must never silently widen what an UNRELATED consumer accepts
/// too, since each consumer's own soundness argument is independently made and independently
/// verified. This enum makes that boundary an explicit, typed parameter rather than a single shared
/// default a later change could accidentally loosen for everyone at once.
///
/// `openspec/changes/plan-construct-coverage-completion` task 4.2 introduces this split: before it,
/// `PatternNode::Segments`/`PatternNode::Anchor` were an unconditional `None` for every caller ALIKE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatternLowerScope {
    /// Every consumer's floor before task 4.2: `PatternNode::Segments`/`PatternNode::Anchor` still
    /// refuse unconditionally (`None`), byte-identical to this crate's pre-4.2 behavior.
    /// [`lower_span`]'s own callers stay on this tier PERMANENTLY by this task's own explicit
    /// ownership boundary -- widening `SimultaneousRewrite`'s own admitted set is a DIFFERENT
    /// characteristic's oracle-verification question (D6), not something a pattern-shape-lowering
    /// change gets to answer as a side effect. `crate::replace::compile_metathesis_rule` also stays
    /// on this tier deliberately (not because it couldn't be widened -- `slot_candidates` would
    /// refuse an `Anchor`/cross-table-`Segments` occurrence independently anyway -- but because
    /// `Metathesis`'s own admitted set is `openspec/changes/plan-construct-coverage-completion` task
    /// 4.6's already-closed row, not this task's to reopen).
    Baseline,
    /// Task 4.2's own widening, for the rewrite-rule compile path
    /// (`crate::replace::compile_rewrite_rule_subset`/`crate::capability::
    /// rtl_reversal_construction_attempted`, which MUST stay in lockstep with each other -- see that
    /// function's own doc): additionally accepts (1) a `PatternNode::Segments` whose OWN declared
    /// table is the SAME table as this call's own `table` parameter (lowers to a literal run of
    /// [`Slot::Fixed`], one per the pre-segmented shape's own interior node, byte-identical to what
    /// an equivalent run of plain `<Segment>` references would produce) -- a Segments node
    /// referencing a DIFFERENT table still refuses, honestly, never silently misinterpreting
    /// another table's char-def id SPACE (a raw `u32` index has no meaning across tables) -- and (2)
    /// any `PatternNode::Anchor` (lowers to [`Slot::Anchor`], rendered as foma's own `.#.`
    /// word-boundary xre atom). A disagree-polarity alpha var and a malformed `Quantifier` still
    /// refuse under this tier too -- widening is strictly ADDITIVE, never a blanket "accept
    /// everything" switch.
    RewriteRuleCompile,
}

/// One position in a rendered pattern.
///
/// `pub(crate)`: this is the canonical definition (moved here, `lower-fst-pattern-environments`
/// Stage 1B migration follow-on) -- `crate::replace` re-exports it at its OLD path
/// (`pub(crate) use crate::lower::Slot;`) so `capability.rs`'s `crate::replace::Slot::Alpha`/
/// `crate::replace::Slot::Repeat` pattern matches and `replace.rs`'s own `slot_candidates`/
/// `reversed_slots`/`compile_rtl_branch_net` keep compiling unmodified.
///
/// `Clone` (`openspec/changes/compile-right-to-left-rewrites`): `replace.rs`'s RTL reversal
/// construction needs a REVERSED copy of a subrule's own slot lists (`reversed_slots`, that
/// file) alongside the original document-order lists it builds the safety-net `LeftToRight`-style
/// branch from -- see that file's `compile_rtl_branch_net` doc.
#[derive(Debug, Clone)]
pub(crate) enum Slot {
    /// A single fixed char-def (`PatternNode::CharDef`, or a `Context` with no alpha vars whose
    /// class happens to be a singleton — kept general as [`Slot::Union`] instead, see below).
    Fixed(CharDefId),
    /// A natural class with no alpha binding at this occurrence: renders as a `[c1|c2|...]` union.
    Union(Vec<CharDefId>),
    /// A natural class occurrence bound to one OR MORE alpha variables (Amharic's CV-merger binds
    /// up to 20 vars on a SINGLE `SimpleContext` — report-08 §3 item 1: "the 20 variables jointly
    /// copy the feature bundles of one (C,V) segment pair"): resolved per-tuple by
    /// [`resolve_alpha_tuples`], not fixed until a concrete assignment is chosen. `occurrence` is
    /// this specific SLOT INSTANCE's own id (unique per occurrence, NOT per variable — two
    /// occurrences of the same [`VarId`] almost always draw from two DIFFERENT classes, e.g.
    /// Indonesian prule4's RHS `nc11` (the nasal output class) vs its right-environment `nc12`
    /// (the following-obstruent class): they must agree on the var's FEATURE VALUE, not resolve
    /// to the identical segment — see [`resolve_alpha_tuples`]'s doc for why this rules out a
    /// same-var-implies-same-segment shortcut). `vars` is one `(VarId, feature lane)` pair per
    /// `AlphaVariable` this ONE occurrence carries — all of them apply to the SAME concrete
    /// segment eventually chosen for this occurrence.
    Alpha {
        vars: Vec<(VarId, pg_grammar::featsys::FlatIndex)>,
        occurrence: usize,
        base_members: Vec<CharDefId>,
    },
    /// `PatternNode::Quantifier { min, max, children }` (`openspec/changes/
    /// compile-bounded-fst-quantifiers` for the originally-supported FINITE case;
    /// `openspec/changes/build-unbounded-quantifier-support` widens this to the genuinely
    /// UNBOUNDED case too): an alpha-free repetition of `children`'s own rendered slots, either
    /// FINITELY bounded (`max: Some(max)`, `min <= max <= MAX_QUANTIFIER_BOUND`) or genuinely
    /// UNBOUNDED (`max: None`, the DTD's `max="-1"` Kleene sentinel). Renders (`render_slots`) as
    /// foma's own NATIVE repetition xre operator — `[<children text>]^{min,max}` for the finite
    /// case, `[<children text>]*`/`[<children text>]^>{min-1}` for the unbounded case (that
    /// function's own doc has the exact operator-selection rule and the off-by-one it depends on)
    /// — never a hand-rolled expansion, so `[Slot::Repeat]`'s compiled size is exactly foma's own
    /// construction: `fsm_concat_m_n` for the finite case (`min` mandatory copies of `children`'s
    /// own compiled sub-net, then `max - min` further copies each individually optional, that
    /// function's own doc, cited in `replace.rs`'s Big-O note) or `fsm_kleene_star`/
    /// `concat(concat_n(net, N), fsm_kleene_plus(net))` for the unbounded case (`foma-0.4.2/src/
    /// regex.rs`'s own `UnaryOp::Star`/`XreExpr::RepeatNPlus` arms) — LINEAR in `min`, and, for the
    /// unbounded case, INDEPENDENT of any repetition count at all (there is none to bound: a native
    /// Kleene star/plus's own compiled net size does not scale with how many times it can match).
    ///
    /// # Why `max: Option<u32>`, not always a concrete bound
    /// `PatternNode::Quantifier`'s own `max` field is already `Option<u32>` (`model.rs`: `None` ⇔
    /// the DTD's `max="-1"` unbounded sentinel, the C# loader's own default — `XmlLanguageLoader.cs`
    /// defaults an absent `max` attribute to `-1`, and the DTD's own `#IMPLIED` doc calls it
    /// "-1 or higher", i.e. unbounded is the DTD's DEFAULT, not an exotic corner). This variant used
    /// to narrow that down to a concrete `u32` because ONLY the finite case compiled
    /// (`compile-bounded-fst-quantifiers`'s own original scope, `slots_from_nodes`'s prior
    /// `let Some(max_v) = max else { return None }` bail). `build-unbounded-quantifier-support`
    /// removes that narrowing: the backend has a native, exact, finite-SIZE construction for the
    /// unbounded case too (`nfst-xre` parses `E^>N`/`E*`; `foma-0.4.2/src/regex.rs`'s own
    /// `RepeatNPlus`/`Star` arms build them with no cutoff anywhere) — refusing it was a SCOPE line
    /// inherited from the original step, never a feasibility finding (that step's own design.md).
    /// [`MAX_QUANTIFIER_BOUND`] applies ONLY when `max` is `Some(_)` (`slots_from_nodes`'s own
    /// Quantifier arm never even evaluates it when `max` is `None`) — an unbounded quantifier's own
    /// compiled net size does not depend on any repetition count, so there is nothing for that
    /// ceiling to bound; treating `max: None` as "a bound above the ceiling" and silently clamping
    /// it to a finite number would be exactly the ADR 0001 violation the original refusal existed
    /// to avoid, and it stays forbidden here too — `max: None` is never coerced to `Some(_)`
    /// anywhere in this module.
    ///
    /// # Why `children: Vec<Slot>`, not a second `Pattern`
    /// `slots_from_nodes` (this variant's own builder) already turns `PatternNode::Quantifier`'s
    /// `children: Vec<PatternNode>` into slots via the IDENTICAL recursive call it uses for the
    /// pattern's own top-level nodes — one node-to-slot mapping, reused, not re-derived (mirrors
    /// this module's "resolve once, reuse everywhere" discipline for `pattern_slots`/
    /// `resolve_alpha_tuples`/`render_slots` themselves). Storing already-resolved `Slot`s (rather
    /// than the raw `PatternNode`s) means [`render_slots`] can render a nested quantifier the SAME
    /// way it renders every other slot list, with no special-cased second PatternNode-to-text path.
    ///
    /// # Why no `Slot::Alpha` may ever appear (transitively) inside `children`
    /// [`slots_from_nodes`] REFUSES (returns `None`) to build a `Slot::Repeat` whose own `children`
    /// contain a `Slot::Alpha` occurrence at ANY nesting depth (checked recursively through any
    /// further-nested `Slot::Repeat`, never just the immediate level) — [`resolve_alpha_tuples`]'s
    /// own occurrence-flattening walks `slot_lists: &[&[Slot]]` at exactly ONE level (the top-level
    /// LHS/RHS/left-env/right-env lists `replace.rs`'s `compile_rewrite_rule_subset`/this module's
    /// own [`lower_span`] pass it), so an `Alpha` occurrence buried inside a `Slot::Repeat`'s own
    /// `children` would never be discovered, never receive a resolved assignment, and would panic
    /// [`render_slots`]'s own `.expect("every alpha slot's occurrence has a resolved assignment by
    /// render time")` the first time anyone tried to render it. Refusing to BUILD the `Slot::Repeat`
    /// in the first place (rather than teaching `resolve_alpha_tuples` to recurse) keeps that
    /// invariant enforced at construction time, not merely by convention — an alpha variable nested
    /// inside a quantifier's own children is therefore honestly out of scope for this change (`None`
    /// from `slots_from_nodes`, exactly like an unbounded quantifier), not a latent panic risk.
    Repeat {
        min: u32,
        max: Option<u32>,
        children: Vec<Slot>,
    },
    /// `PatternNode::Anchor` (`initialBoundaryCondition`/`finalBoundaryCondition` on a
    /// `<PhoneticTemplate>`, or a bare leading/trailing `#` in an environment string --
    /// `openspec/changes/plan-construct-coverage-completion` task 4.2): accepted only under
    /// [`PatternLowerScope::RewriteRuleCompile`] ([`PatternLowerScope`]'s own doc). Renders
    /// ([`render_slots`]) as foma's own `.#.` xre atom -- "signifies both end and beginning of
    /// word/string" (`foma-0.4.2/src/iface/print.rs`'s own built-in help text for the operator,
    /// `regex.rs`'s `XreExpr::BoundaryMarker => fsm_symbol(".#.")`) -- IDENTICALLY regardless of
    /// which [`AnchorSide`] this occurrence carries: the compiled meaning ("start of word" vs "end
    /// of word") comes entirely from WHICH SIDE of the rule's own `_` focus marker the rendered text
    /// sits on (leading in a left environment = word-initial; trailing in a right environment =
    /// word-final -- `pg_grammar::compile::rules.rs`'s own construction: `Anchor(Left)` is always
    /// PREPENDED to `left_env`, `Anchor(Right)` always APPENDED to `right_env`), never from the tag
    /// itself. This is exactly what makes [`crate::replace::compile_rtl_branch_net`]'s existing
    /// mirror-and-reverse construction swap an anchor to the CORRECT opposite edge with NO
    /// anchor-specific code in that function at all: `reversed_slots` reverses this slot's own
    /// POSITION within its containing environment list (an atomic slot, no internal reversal, same
    /// as [`Slot::Fixed`]/[`Slot::Union`]) and swaps left_env<->right_env wholesale, so a
    /// `Right`-anchor that was the LAST slot of the original `right_env` becomes the FIRST slot of
    /// the mirror's own `left_env` -- rendered as a LEADING `.#.` there, meaning "start of the
    /// mirror/reversed representation", which `fsm_reverse` then correctly turns into "end of the
    /// real string" for the final network (the SAME "reversing a network that operates on reversed
    /// strings gives back a network operating on normal strings" argument the module's own top doc
    /// already makes for ordinary content, here applied to a boundary symbol instead of a
    /// character). Pinned empirically, not just argued: `tests/phase_c_right_to_left.rs`'s
    /// `rtl_anchor_reversal_swaps_the_correct_edge`.
    ///
    /// `#[allow(dead_code)]`: the carried [`AnchorSide`] is never actually READ by this crate today
    /// (this doc's own argument for why [`render_slots`] can render `.#.` unconditionally,
    /// regardless of side) -- kept anyway, not collapsed to a unit variant, because it is real
    /// structural information a future caller (a diagnostic, or a stricter position-validity check)
    /// may legitimately want, and because `PatternNode::Anchor(AnchorSide)` (the node this variant
    /// mirrors) carries it too — dropping it here would be a lossy projection for no code-size
    /// benefit worth mentioning.
    #[allow(dead_code)]
    Anchor(AnchorSide),
}

/// `true` iff `slots` (or the `children` of any `Slot::Repeat` nested at ANY depth inside `slots`)
/// contains at least one `Slot::Alpha` occurrence — [`Slot::Repeat`]'s own doc explains why a
/// `Slot::Repeat` may never be built over such `children`: this is the recursive check
/// [`slots_from_nodes`]'s own `PatternNode::Quantifier` arm uses to enforce that, checked at EVERY
/// nesting depth (not just the immediate one) so a nested bounded-quantifier-inside-a-bounded-
/// quantifier can never smuggle an alpha occurrence past a shallow, single-level check.
fn slots_contain_alpha(slots: &[Slot]) -> bool {
    slots.iter().any(|s| match s {
        Slot::Alpha { .. } => true,
        Slot::Repeat { children, .. } => slots_contain_alpha(children),
        Slot::Fixed(_) | Slot::Union(_) | Slot::Anchor(_) => false,
    })
}

/// Preflight ceiling on a [`PatternNode::Quantifier`]'s own FINITE `max` bound (`openspec/changes/
/// compile-bounded-fst-quantifiers`, design.md: "Preflight the product of alternatives/repetitions
/// and report a typed budget or unsupported result"). Checked in `slots_from_nodes` BEFORE any xre
/// text is rendered or any `Fsm` is built at all — the cheapest possible predictor, the same "check
/// the search result before the expensive part" principle `resolve_alpha_tuples`' own V3 alpha-tuple
/// cap uses. `pattern_slots`/`slots_from_nodes` are pure structural walks with no
/// [`crate::compose_budget::ComposeBudget`] threaded through them (a fixed, always-on structural
/// ceiling rather than a new env-configurable budget dimension) — a finite `max` above this ceiling
/// is honestly reported unsupported (`None`), never silently clamped down to it (that would be
/// exactly the finite-cutoff-masquerading-as-something-else move ADR 0001 forbids, just at a
/// different bound). Generous relative to any authored HC grammar this crate has ever seen
/// (`OptionalSegmentSequence` bounds in the reference/synthetic grammars are single digits) while
/// keeping even an UNCHECKED first branch net trivially bounded before any `ComposeBudget` size
/// check ever runs.
///
/// **Never applied to a genuinely UNBOUNDED quantifier** (`openspec/changes/
/// build-unbounded-quantifier-support`): `max: None` is not "a bound above this ceiling" — it is a
/// DIFFERENT construction entirely (foma's native `E*`/`E^>N` Kleene star/plus, [`Slot::Repeat`]'s
/// own doc), whose compiled net size does not scale with any repetition count at all, so there is
/// nothing here for this ceiling to usefully bound. `slots_from_nodes`'s own Quantifier arm never
/// evaluates this constant when `max` is `None` — silently coercing an unbounded quantifier into a
/// finite one just to run this check would itself be the ADR 0001 violation this ceiling exists to
/// prevent for the FINITE case, so that path is never taken.
const MAX_QUANTIFIER_BOUND: u32 = 512;

/// Walk `pattern`'s nodes into [`Slot`]s, numbering each `Alpha` occurrence sequentially from
/// `*next_occurrence` (shared across LHS/RHS/left-env/right-env for one subrule — see
/// `replace.rs`'s `compile_rewrite_rule`, or this module's own [`lower_span`], which resets its own
/// FRESH counter per span). Returns `None` (uncovered) on a disagree-polarity `Context`; an
/// out-of-scope `Quantifier` (inverted/over-budget-finite/alpha-nested/empty-children — see
/// [`Slot::Repeat`]'s own doc; a genuinely UNBOUNDED quantifier is no longer, by itself, out of
/// scope, `openspec/changes/build-unbounded-quantifier-support`); or, when `scope` is
/// [`PatternLowerScope::Baseline`], any `Segments`/`Anchor` node at all (when `scope` is
/// [`PatternLowerScope::RewriteRuleCompile`], a `Segments` node referencing a DIFFERENT table than
/// `table` still refuses, but a same-table `Segments` and any `Anchor` now lower successfully --
/// `openspec/changes/plan-construct-coverage-completion` task 4.2, [`PatternLowerScope`]'s own doc).
///
/// `table`: every `Context` node's `NatClassId` is resolved against THIS table
/// ([`class_members`]), never an implicit grammar-wide default
/// (`openspec/changes/fix-multitable-fst-compilation`, design.md: "table zero is never an
/// implicit default"). The caller is responsible for choosing the RIGHT table — see
/// [`crate::replace::owning_table`]'s own doc for how `replace.rs`'s `compile_rewrite_rule_subset`
/// picks it (the rule's own stratum's `StratumDef::table`), and [`lower_span`]'s own call sites for
/// how THIS module picks it (`alphabet.table()`, already the correct per-caller table by that
/// function's own contract). A `PatternNode::Segments`' OWN declared table is compared against THIS
/// SAME `table` by pointer identity (`std::ptr::eq`, both being borrowed from the same `g.char_tables`
/// vec this pattern's own grammar owns) -- cheap, exact, and needs no new `TableId`-threading
/// through this function's signature.
///
/// `pub(crate)`: canonical definition (moved here, migration follow-on) -- `replace.rs`
/// re-exports it at its OLD path so `capability.rs`'s structural probes and every existing
/// `crate::replace::pattern_slots`/`pg_foma::replace::pattern_slots` caller keep compiling
/// unmodified.
pub(crate) fn pattern_slots(
    g: &Grammar,
    table: &CharDefTable,
    pattern: &Pattern,
    next_occurrence: &mut usize,
    scope: PatternLowerScope,
) -> Option<Vec<Slot>> {
    slots_from_nodes(g, table, &pattern.nodes, next_occurrence, scope)
}

/// [`pattern_slots`]'s own per-node walk, factored out over a bare node slice (rather than a whole
/// `&Pattern`) so [`PatternNode::Quantifier`]'s own `children` (`openspec/changes/
/// compile-bounded-fst-quantifiers`) can recurse through the IDENTICAL per-node semantics
/// `pattern_slots` already gives a whole pattern — one pattern-node-to-slot mapping, not two
/// independently-maintained ones (mirrors this module's own "one shared occurrence counter"
/// discipline for LHS/RHS/environment: `next_occurrence` threads through this recursion exactly
/// like it already threads across a subrule's LHS/RHS/left-env/right-env calls). `scope` threads
/// through the SAME way, unchanged across the whole recursion -- a `Quantifier`'s own `children`
/// never gets a more permissive (or stricter) scope than its parent pattern.
fn slots_from_nodes(
    g: &Grammar,
    table: &CharDefTable,
    nodes: &[PatternNode],
    next_occurrence: &mut usize,
    scope: PatternLowerScope,
) -> Option<Vec<Slot>> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            PatternNode::CharDef(id) => out.push(Slot::Fixed(*id)),
            PatternNode::Context(sc) => {
                if sc.vars.is_empty() {
                    let members = class_members(g, table, sc.nat_class, &HashSet::new());
                    out.push(Slot::Union(members));
                } else {
                    if sc.vars.iter().any(|v| !v.plus) {
                        // "disagree" polarity — documented gap, never seen in the reference
                        // grammars (module doc).
                        return None;
                    }
                    let excl: HashSet<usize> =
                        sc.vars.iter().map(|v| v.feature.0 as usize).collect();
                    let base = class_members(g, table, sc.nat_class, &excl);
                    let occurrence = *next_occurrence;
                    *next_occurrence += 1;
                    let vars = sc.vars.iter().map(|v| (v.var, v.feature)).collect();
                    out.push(Slot::Alpha {
                        vars,
                        occurrence,
                        base_members: base,
                    });
                }
            }
            PatternNode::Quantifier { min, max, children } => {
                // `openspec/changes/build-unbounded-quantifier-support`: a genuinely unbounded
                // quantifier (`max == None`, the DTD's `max="-1"` Kleene sentinel) is ACCEPTED here
                // now -- it has its own native, exact, finite-SIZE foma construction (`render_slots`'
                // own doc: `E*`/`E^>N`), so refusing it was a scope line, not a feasibility finding.
                // The checks below (inverted bound, `MAX_QUANTIFIER_BOUND` preflight) apply ONLY to
                // a FINITE bound -- neither is well-formed to ask of `None` (there is no upper value
                // to compare, and no repetition count for the ceiling to bound, `Slot::Repeat`'s own
                // doc) -- so both are skipped entirely for the unbounded case; `max` is never
                // silently coerced to a concrete number to force them to run (ADR 0001: a finite
                // cutoff must never masquerade as unbounded semantics).
                if let Some(max_v) = max {
                    // Inverted bound -- no sound finite construction exists for it; conservative
                    // honest-unsupported rather than silently swapping/clamping min/max.
                    if min > max_v {
                        return None;
                    }
                    // Preflight (design.md: "Preflight the product of alternatives/repetitions and
                    // report a typed budget or unsupported result") -- checked BEFORE recursing into
                    // children/rendering any xre text at all, the cheapest possible predictor.
                    if *max_v > MAX_QUANTIFIER_BOUND {
                        return None;
                    }
                }
                let child_slots = slots_from_nodes(g, table, children, next_occurrence, scope)?;
                if child_slots.is_empty() {
                    // No renderable child at all (an empty <OptionalSegmentSequence>) -- not a
                    // shape any DTD-legal grammar this crate has seen produces; nothing to
                    // bound-repeat, so honest-unsupported rather than rendering a vacuous group.
                    return None;
                }
                if slots_contain_alpha(&child_slots) {
                    // Alpha-bound occurrence nested inside a quantifier group -- out of scope for
                    // this change (`Slot::Repeat`'s own doc: `resolve_alpha_tuples` does not
                    // recurse into a `Slot::Repeat`'s own children) -- honest-unsupported rather
                    // than risk an unresolved occurrence panicking at render time.
                    return None;
                }
                out.push(Slot::Repeat {
                    min: *min,
                    max: *max,
                    children: child_slots,
                });
            }
            PatternNode::Segments {
                table: seg_table_id,
                shape,
            } => {
                if scope != PatternLowerScope::RewriteRuleCompile {
                    return None;
                }
                // Cross-table `Segments`: the node's own declared table differs from the table
                // THIS call is lowering against -- a raw `char_def: u32` index has no meaning
                // across two different `CharDefTable`s' own `defs` vecs (chardef.rs's own
                // `CharDefTable::get`: `&self.defs[id.0 as usize]`, silently wrong or panicking if
                // misapplied to another table), so this stays honestly out of scope rather than
                // risk misinterpreting another table's char-def id space (`PatternLowerScope`'s own
                // doc). Pointer identity is exact here: both references are borrowed from the SAME
                // `g.char_tables` vec this pattern's own grammar owns.
                let seg_table = &g.char_tables[seg_table_id.0 as usize];
                if !std::ptr::eq(seg_table, table) {
                    return None;
                }
                // Same-table: a pre-segmented literal shape lowers to one `Slot::Fixed` per
                // interior node (bridge.rs's own `PatternNode::Segments` handling does the
                // identical `shape.shape.interior()` walk for its own, differently-shaped FST
                // backend -- this is not a re-derivation of new segmentation logic, just this
                // module's OWN `Slot` vocabulary applied to the same already-segmented data).
                // `interior()` already excludes the two bracketing anchor nodes `pg_shape::Shape`
                // wraps every shape in (its own doc: "everything but the two anchors") -- an
                // entirely different, lower-level concept than this module's own `Slot::Anchor`
                // (a grammar-level `PatternNode::Anchor` word-boundary CONDITION), never conflated.
                for (_, _kind, char_def, _flags) in shape.shape.interior() {
                    out.push(Slot::Fixed(CharDefId(char_def)));
                }
            }
            PatternNode::Anchor(side) => {
                if scope != PatternLowerScope::RewriteRuleCompile {
                    return None;
                }
                out.push(Slot::Anchor(*side));
            }
        }
    }
    Some(out)
}

// =================================================================================================
// Alpha-tuple resolution (reports/08 §3.1): cartesian product per variable, filtered by joint
// agreement, generic over N variables / N slots-per-variable.
//
// MOVED HERE from `replace.rs` (migration follow-on, module top doc) -- logic byte-for-byte
// unchanged, `replace.rs` re-exports `AlphaAssignment`/`TupleReport`/`resolve_alpha_tuples` at
// their old paths.
// =================================================================================================

/// One assignment of every alpha slot OCCURRENCE (module doc on [`Slot::Alpha`] — keyed by
/// occurrence id, NOT by [`VarId`]: two occurrences of the same variable generally resolve to
/// two DIFFERENT concrete segments, e.g. prule4's nasal-output segment and its
/// following-obstruent segment, which merely need to AGREE on the variable's feature value, not
/// be the same segment) to a concrete [`CharDefId`], surviving the joint agreement filter.
pub struct AlphaAssignment {
    pub values: std::collections::HashMap<usize, CharDefId>,
}

/// Report for one alpha-bearing subrule: the naive per-slot product size (what a per-variable-name
/// expander would enumerate before any filtering) vs. the number of tuples surviving the joint
/// agreement constraint (reports/08's "count of segment tuples satisfying the joint constraint").
#[derive(Debug, Clone, Copy)]
pub struct TupleReport {
    pub raw_product: usize,
    pub surviving: usize,
}

/// Locate every [`Slot::Alpha`] occurrence across `slot_lists` (one `Vec<Slot>` per pattern zone:
/// LHS, RHS, left-env, right-env — in that order, any of which may be empty), and enumerate the
/// surviving tuple-indexed cross product: the FULL product of every occurrence's OWN candidate
/// set (never a same-var intersection — see [`AlphaAssignment`]'s doc for why that shortcut is
/// wrong), filtered to combinations where every pair of occurrences sharing a [`VarId`] AGREES —
/// unify (bitwise-overlap, matching this codebase's own natural-class-membership idiom, not
/// strict equality, since an underspecified segment's lane can carry more than one live bit) — at
/// that variable's feature lane. This is reports/08 §3.1's "count of segment tuples satisfying
/// the joint constraint" bound (Amharic's 20-var CV-merger: nc15=59 × nc16=6 ⇒ ≤354, never v^20),
/// implemented generically over N variables and N occurrences per variable. Returns
/// `(assignments, report)`; a rule with zero alpha slots returns one trivial
/// `AlphaAssignment { values: {} }` and a `raw_product`/`surviving` of 1 (nothing to expand).
///
/// `table`: every alpha occurrence's feature-lane agreement test (`lane_value`, below) resolves
/// against THIS table, never an implicit `g.char_tables[0]` default
/// (`openspec/changes/fix-multitable-fst-compilation` — the second of the two hardcoded sites that
/// change's design.md names, alongside [`pattern_slots`]'s own former `table_of` call). The
/// `members: Vec<CharDefId>` each [`Slot::Alpha`] already carries were themselves resolved against
/// this SAME table by [`pattern_slots`] (the caller's job: pass ONE consistent table to both), so
/// this function's own `table` parameter must be the identical table [`pattern_slots`] used to
/// build `slot_lists` in the first place — never a second, independently-chosen one.
///
/// `pub(crate)`: canonical definition (moved here, migration follow-on) -- `replace.rs`
/// re-exports it at its OLD path (`pub(crate) use crate::lower::resolve_alpha_tuples;`) so its own
/// `compile_rewrite_rule_subset` and every other existing caller keep compiling unmodified.
pub(crate) fn resolve_alpha_tuples(
    table: &CharDefTable,
    slot_lists: &[&[Slot]],
) -> (Vec<AlphaAssignment>, TupleReport) {
    // Flatten to (occurrence, vars, members), document order (deterministic, not semantically
    // load-bearing), plus the var-group membership needed for the filter step. One occurrence may
    // carry MANY (var, feature) pairs at once (Amharic's CV-merger: up to 20 on one node) — all of
    // them constrain the SAME concrete segment this occurrence resolves to.
    struct Occ {
        id: usize,
        vars: Vec<(VarId, pg_grammar::featsys::FlatIndex)>,
        members: Vec<CharDefId>,
    }
    let mut occs: Vec<Occ> = Vec::new();
    for slots in slot_lists {
        for slot in slots.iter() {
            if let Slot::Alpha {
                vars,
                occurrence,
                base_members,
            } = slot
            {
                occs.push(Occ {
                    id: *occurrence,
                    vars: vars.clone(),
                    members: base_members.clone(),
                });
            }
        }
    }
    if occs.is_empty() {
        return (
            vec![AlphaAssignment {
                values: std::collections::HashMap::new(),
            }],
            TupleReport {
                raw_product: 1,
                surviving: 1,
            },
        );
    }
    occs.sort_by_key(|o| o.id);

    let raw_product: usize = occs.iter().map(|o| o.members.len().max(1)).product();

    // Cross product across ALL occurrences (each occurrence independently ranges over its own
    // candidate set).
    let mut assignments: Vec<std::collections::HashMap<usize, CharDefId>> =
        vec![std::collections::HashMap::new()];
    for occ in &occs {
        let mut next = Vec::with_capacity(assignments.len() * occ.members.len().max(1));
        for asg in &assignments {
            for &cd in &occ.members {
                let mut a = asg.clone();
                a.insert(occ.id, cd);
                next.push(a);
            }
        }
        assignments = next;
    }

    // Joint-agreement filter: for every pair of occurrences sharing a VarId, the two chosen
    // segments must unify (bitwise overlap) at that variable's feature lane. An occurrence with
    // MULTIPLE vars contributes one entry per var it carries.
    let mut var_pairs: std::collections::HashMap<
        VarId,
        Vec<(usize, pg_grammar::featsys::FlatIndex)>,
    > = std::collections::HashMap::new();
    for occ in &occs {
        for &(var, feature) in &occ.vars {
            var_pairs.entry(var).or_default().push((occ.id, feature));
        }
    }
    let lane_value = |cd: CharDefId, feature: pg_grammar::featsys::FlatIndex| -> u64 {
        table.get(cd).feature_lanes()[feature.0 as usize]
    };
    let survivors: Vec<AlphaAssignment> = assignments
        .into_iter()
        .filter(|asg| {
            var_pairs.values().all(|occs_for_var| {
                occs_for_var.iter().all(|&(id_a, feat)| {
                    occs_for_var.iter().all(|&(id_b, _)| {
                        let a = lane_value(asg[&id_a], feat);
                        let b = lane_value(asg[&id_b], feat);
                        a & b != 0
                    })
                })
            })
        })
        .map(|values| AlphaAssignment { values })
        .collect();

    let surviving = survivors.len();
    (
        survivors,
        TupleReport {
            raw_product,
            surviving,
        },
    )
}

// =================================================================================================
// Slot -> regex text (given a concrete alpha assignment).
//
// MOVED HERE from `replace.rs` (migration follow-on, module top doc) -- logic byte-for-byte
// unchanged, `replace.rs` re-exports `render_slots` at its old path.
// =================================================================================================

/// Renders `slots` to xre source text, ONE SPACE between consecutive slots (never omitted — see
/// the note below on why). A single space also separates union members inside one `[...]` group,
/// same reason.
///
/// **Load-bearing finding (prototype report):** this vendored foma-rs's xre lexer (`nfst-xre`)
/// does NOT reliably treat two ADJACENT non-ASCII (here: Private-Use-Area) codepoints written
/// back-to-back with NO separator as two independent single-symbol atoms — confirmed by direct
/// bisection (`examples/p6_bisect.rs`): `"t -> 0 || e n + _ u"` (PUA tokens, SPACE-separated)
/// correctly deletes in context; the byte-identical rule written as `"e _ +t"` (the boundary and
/// following consonant tokens concatenated with NO space) silently fails to match — no parse
/// error, no panic, just a rule that never fires, the worst kind of failure to debug blind. ASCII
/// letters tolerate bare concatenation fine (`"cat"` == `"c a t"`, both split per-character,
/// verified by the vendored crate's own tests) — the gap is specific to non-ASCII/high-codepoint
/// symbols, which is exactly what a char-def-identity token alphabet is built from. Mainline P6
/// must carry this forward as a hard rule for ANY xre string this compiler emits.
///
/// `pub(crate)`: canonical definition (moved here, migration follow-on) -- `replace.rs`
/// re-exports it at its old path so `replace.rs`'s own `render_branch_regex` and every other
/// existing caller keep compiling unmodified.
/// Render an already-deduplicated token set as one atom: a bare char for a singleton (the ordinary
/// case, and the ONLY case for every pre-aliasing caller/grammar), or foma's own `[a | b | ...]`
/// union syntax for two-or-more — the exact same shape [`render_slots`]'s `Slot::Union` arm always
/// rendered for a multi-member natural class, reused here so an aliased [`Slot::Fixed`] atom or an
/// aliased-and-unioned [`Slot::Union`] atom look identical to a caller that never observes aliasing
/// at all.
fn format_union_tokens(chars: &[char]) -> String {
    if chars.len() == 1 {
        chars[0].to_string()
    } else {
        let inner: Vec<String> = chars.iter().map(|c| c.to_string()).collect();
        format!("[{}]", inner.join(" | "))
    }
}

pub(crate) fn render_slots(
    alphabet: &SegAlphabet,
    slots: &[Slot],
    assignment: &AlphaAssignment,
) -> String {
    let mut pieces: Vec<String> = Vec::with_capacity(slots.len());
    for slot in slots {
        let piece = match slot {
            // `Slot::Fixed`/`Slot::Union`: render-time cross-table alias expansion
            // (`docs/conformance/multitable-shared-representation-design.md` item 3 — "in
            // `lower::render_slots`' `Slot::Fixed`/`Slot::Union` arms, NOT in `class_members`").
            // `alphabet.render_tokens(cd)` is exactly `vec![alphabet.token(cd)]` (byte-identical
            // to the pre-aliasing rendering below) whenever `alphabet` carries no table identity
            // (every existing caller, built via `SegAlphabet::new`) or `cd`'s spelling is unique to
            // its own table — `format_union_tokens` degenerates to the same bare-char/`[a | b]`
            // shapes this function always rendered. `Slot::Alpha` deliberately does NOT alias here
            // (design's own scope: only the two class/fixed-atom arms) — an alpha occurrence's
            // resolved segment is chosen per-tuple from `class_members`' own single-table
            // resolution, out of this step's scope.
            Slot::Fixed(cd) => format_union_tokens(&alphabet.render_tokens(*cd)),
            Slot::Union(members) => {
                let mut chars: Vec<char> = Vec::with_capacity(members.len());
                for m in members {
                    for c in alphabet.render_tokens(*m) {
                        if !chars.contains(&c) {
                            chars.push(c);
                        }
                    }
                }
                format_union_tokens(&chars)
            }
            Slot::Alpha { occurrence, .. } => {
                let cd = assignment.values.get(occurrence).expect(
                    "every alpha slot's occurrence has a resolved assignment by render time",
                );
                alphabet.token(*cd).to_string()
            }
            Slot::Repeat { min, max, children } => {
                // Recurses into `render_slots` for `children` -- the SAME rendering, same PUA-token
                // space, same load-bearing space-separation rule this whole function's own doc
                // already establishes; no second text-rendering path, for either arm below.
                let inner = render_slots(alphabet, children, assignment);
                match max {
                    // `openspec/changes/compile-bounded-fst-quantifiers` ("Bounded quantifiers"):
                    // foma's own native bounded-repetition xre operator, `^{min,max}` (`nfst-xre`'s
                    // `CatenateNToK`, lexed as a POSTFIX operator over whatever `[...]`-grouped term
                    // precedes it -- `[...]` is foma's plain GROUPING bracket, distinct from `(...)`'s
                    // OPTIONALITY meaning `replace.rs`'s own `render_branch_regex` relies on for
                    // epenthesis).
                    Some(max_v) => format!("[{inner}]^{{{min},{max_v}}}"),
                    // `openspec/changes/build-unbounded-quantifier-support`: `max: None`, the DTD's
                    // `max="-1"` Kleene sentinel. `min == 0` ("zero or more") is foma's plain `*`
                    // postfix (`nfst-xre`'s `Token::Star`, `UnaryOp::Star` -> `fsm_kleene_star`).
                    // `min >= 1` ("`min` or more") needs `nfst-xre`'s `E^>N` ("MORENCONCAT",
                    // `CatenateNPlus`) -- **load-bearing off-by-one**: `foma-0.4.2/src/regex.rs`'s own
                    // `XreExpr::RepeatNPlus(inner, n)` arm builds `concat(concat_n(net, n),
                    // kleene_plus(net))`, i.e. `n` mandatory copies followed by ONE OR MORE further
                    // copies -- `n` mandatory + >=1 more = STRICTLY MORE THAN `n` copies, i.e. `n+1`
                    // copies or more. So `^>N` means "more than N", NOT "N or more": rendering
                    // `min` "or more" therefore requires `^>(min-1)`, never `^>min` (which would
                    // wrongly demand `min+1` or more, off by one) -- pinned by
                    // `render_slots_unbounded_min_off_by_one_boundary` (this module's own test
                    // module), which distinguishes `min` occurrences (must match) from `min-1`
                    // (must not).
                    None if *min == 0 => format!("[{inner}]*"),
                    None => format!("[{inner}]^>{}", min - 1),
                }
            }
            // `openspec/changes/plan-construct-coverage-completion` task 4.2: foma's own `.#.`
            // word-boundary xre atom, IDENTICAL text regardless of `AnchorSide` -- [`Slot::Anchor`]'s
            // own doc has the full argument for why the side never needs to be inspected here (the
            // rendered POSITION, not the tag, is what conveys "word-initial" vs "word-final").
            Slot::Anchor(_) => ".#.".to_string(),
        };
        pieces.push(piece);
    }
    pieces.join(" ")
}

/// A pattern node kind [`lower_span`] cannot yet represent (design.md `spec.md`'s "typed
/// unsupported disposition... does not omit or weaken the node"). Named after the `model.rs`
/// [`PatternNode`] variant (or, for the one non-node case, the [`pg_grammar::model::AlphaVar`]
/// shape) it names, so a caller's diagnostic can cite the EXACT construct rather than a generic
/// "pattern too complex" message — exactly the naming [`crate::replace::pattern_slots`]'s own doc
/// already scopes as unrendered by this crate's existing pattern compiler, carried through here as
/// a typed value instead of a silent `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedPatternNode {
    /// `PatternNode::Quantifier` (`<OptionalSegmentSequence min max>`) that
    /// [`crate::replace::pattern_slots`] still refuses: genuinely UNBOUNDED (`max == None`),
    /// inverted (`min > max`), pathologically large (past
    /// [`crate::replace`]'s own preflight bound), carrying an alpha-bound occurrence anywhere in
    /// its own children (`openspec/changes/compile-bounded-fst-quantifiers`, that module's own
    /// `Slot::Repeat` doc names exactly this scope line), or with no renderable child at all. A
    /// FINITELY bounded, alpha-free quantifier no longer reaches this variant at all — `pattern_slots`
    /// accepts it directly (a new `Slot::Repeat`), so [`lower_span`] lowers it transparently, same as
    /// any other supported node.
    Quantifier,
    /// `PatternNode::Segments` (`<Segments><PhoneticShape>`) — an inline pre-segmented literal shape
    /// group. Under [`PatternLowerScope::Baseline`] ANY `Segments` node triggers this (unchanged,
    /// pre-4.2 behavior); under [`PatternLowerScope::RewriteRuleCompile`]
    /// (`openspec/changes/plan-construct-coverage-completion` task 4.2) a SAME-table `Segments` no
    /// longer reaches this variant at all (it lowers to a literal run of `Slot::Fixed`) — only a
    /// `Segments` node referencing a DIFFERENT table than the pattern's own still does, since a raw
    /// char-def index has no meaning across two tables' own id spaces.
    Segments,
    /// `PatternNode::Anchor` (`initialBoundaryCondition`/`finalBoundaryCondition`, or a bare
    /// leading/trailing `#` in an environment string) — a word-boundary condition. Under
    /// [`PatternLowerScope::Baseline`] this triggers unconditionally (unchanged, pre-4.2 behavior);
    /// under [`PatternLowerScope::RewriteRuleCompile`] (task 4.2) `Anchor` no longer reaches this
    /// variant AT ALL — it always lowers to [`Slot::Anchor`] instead.
    Anchor,
    /// A `PatternNode::Context` carrying an [`pg_grammar::model::AlphaVar`] with `plus == false`
    /// ("disagree" polarity) — not a distinct node KIND, but the same "cannot lower faithfully"
    /// outcome [`pattern_slots`] already reports as unrendered. Scope-independent: no
    /// [`PatternLowerScope`] tier accepts this shape (`openspec/changes/
    /// plan-construct-coverage-completion` task 4.2 deliberately leaves it refused — an orthogonal,
    /// pre-existing gap in `resolve_alpha_tuples`' own joint-agreement filter, unrelated to
    /// direction/reversal, and out of this task's own scope; see that task's final report for the
    /// full reasoning).
    AlphaDisagreePolarity,
}

impl std::fmt::Display for UnsupportedPatternNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            UnsupportedPatternNode::Quantifier => "Quantifier (OptionalSegmentSequence)",
            UnsupportedPatternNode::Segments => "Segments (inline PhoneticShape group)",
            UnsupportedPatternNode::Anchor => "Anchor (word-boundary condition)",
            UnsupportedPatternNode::AlphaDisagreePolarity => {
                "Context with a disagree-polarity AlphaVariable"
            }
        };
        f.write_str(label)
    }
}

/// Scans `pattern` for the FIRST node [`pattern_slots`] (called with this SAME `g`/`table`/`scope`)
/// cannot lower, to recover a typed reason after `pattern_slots` has already returned `None` for it.
/// `pub(crate)` (`openspec/changes/plan-construct-coverage-completion` task 4.2): exposed so
/// `capability.rs`'s `RightToLeftRewriteFaithfulReversalPredicate` can name the EXACT failing shape
/// in its own `Refuse` witness, rather than a laundry-list "could be any of these" message — the
/// task's own "make the predicate's witness name that specific shape" requirement.
///
/// Recurses into a `Quantifier`'s own `children` ([`diagnose_unsupported_nodes`]) rather than
/// assuming the FIRST `Quantifier` node encountered is automatically the culprit: a well-formed
/// quantifier earlier in document order than the REAL failing node would otherwise be mis-blamed
/// (this function's own precision bar, matching [`slots_from_nodes`]'s actual accept/reject order
/// exactly — a diagnostic that mis-names its cause is worse than no diagnostic).
///
/// Never called on a pattern `pattern_slots` actually accepted for this SAME `scope` — `unreachable!`
/// guards that invariant rather than silently reporting a wrong/default reason.
pub(crate) fn diagnose_unsupported(
    g: &Grammar,
    table: &CharDefTable,
    pattern: &Pattern,
    scope: PatternLowerScope,
) -> UnsupportedPatternNode {
    diagnose_unsupported_nodes(g, table, &pattern.nodes, scope).unwrap_or_else(|| {
        unreachable!(
            "pg_foma::lower::diagnose_unsupported called on a pattern pattern_slots did not \
             actually reject under scope {scope:?}: {pattern:?} (a caller bug, not a \
             grammar-authoring one)"
        )
    })
}

/// `true` iff `nodes` (at ANY nesting depth through a `Quantifier`'s own `children`) contains a
/// `PatternNode::Context` carrying at least one `AlphaVar` — the same "would this node list produce
/// a `Slot::Alpha` occurrence somewhere" question [`slots_contain_alpha`] asks of an already-lowered
/// `&[Slot]`, re-derived here at the `PatternNode` level (before lowering) so
/// [`diagnose_unsupported_nodes`]'s own `Quantifier` arm can check it WITHOUT first building
/// `Slot`s for a subtree it may still end up rejecting for an entirely different reason.
/// Disagree-polarity vars are covered too (a disagree-polarity `Context` still carries a non-empty
/// `vars` list) — harmless double-coverage, since [`diagnose_unsupported_nodes`]'s own `Context` arm
/// always reports the MORE SPECIFIC `AlphaDisagreePolarity` reason first, before this function is
/// ever consulted for that same node.
fn nodes_contain_alpha_context(nodes: &[PatternNode]) -> bool {
    nodes.iter().any(|n| match n {
        PatternNode::Context(sc) => !sc.vars.is_empty(),
        PatternNode::Quantifier { children, .. } => nodes_contain_alpha_context(children),
        PatternNode::CharDef(_) | PatternNode::Segments { .. } | PatternNode::Anchor(_) => false,
    })
}

/// [`diagnose_unsupported`]'s own recursive walk, mirroring [`slots_from_nodes`]'s EXACT accept/
/// reject decisions node-by-node (never a re-derived, independently-drifting approximation) so the
/// reason it reports is always the REAL one. Returns `None` when `nodes` is (as far as this function
/// can tell) fully lowerable — the top-level [`diagnose_unsupported`] treats that as a caller-bug
/// `unreachable!`, since it is only ever invoked after `pattern_slots` has already rejected this
/// exact `nodes` list.
fn diagnose_unsupported_nodes(
    g: &Grammar,
    table: &CharDefTable,
    nodes: &[PatternNode],
    scope: PatternLowerScope,
) -> Option<UnsupportedPatternNode> {
    for node in nodes {
        match node {
            PatternNode::CharDef(_) => {}
            PatternNode::Context(sc) => {
                if sc.vars.iter().any(|v| !v.plus) {
                    return Some(UnsupportedPatternNode::AlphaDisagreePolarity);
                }
            }
            PatternNode::Quantifier { min, max, children } => {
                if let Some(max_v) = max {
                    if min > max_v || *max_v > MAX_QUANTIFIER_BOUND {
                        return Some(UnsupportedPatternNode::Quantifier);
                    }
                }
                if children.is_empty() {
                    return Some(UnsupportedPatternNode::Quantifier);
                }
                // A well-formed quantifier's own children might STILL hide the true failing node
                // (a nested Segments/Anchor/disagree-polarity var, or a nested malformed
                // quantifier) -- recurse rather than assume THIS quantifier is the culprit just
                // because it is the first one seen.
                if let Some(reason) = diagnose_unsupported_nodes(g, table, children, scope) {
                    return Some(reason);
                }
                // Children lower cleanly on their own, but an alpha-bound occurrence anywhere
                // inside them still makes the OUTER `Slot::Repeat` unbuildable
                // (`slots_from_nodes`'s own final `slots_contain_alpha(&child_slots)` check) --
                // the true reason in that case IS this quantifier (a well-formed quantifier that
                // simply may never wrap an alpha occurrence).
                if nodes_contain_alpha_context(children) {
                    return Some(UnsupportedPatternNode::Quantifier);
                }
            }
            PatternNode::Segments {
                table: seg_table_id,
                ..
            } => {
                if scope != PatternLowerScope::RewriteRuleCompile {
                    return Some(UnsupportedPatternNode::Segments);
                }
                let seg_table = &g.char_tables[seg_table_id.0 as usize];
                if !std::ptr::eq(seg_table, table) {
                    return Some(UnsupportedPatternNode::Segments);
                }
            }
            PatternNode::Anchor(_) => {
                if scope != PatternLowerScope::RewriteRuleCompile {
                    return Some(UnsupportedPatternNode::Anchor);
                }
            }
        }
    }
    None
}

/// Compiles `text` to an [`Fsm`] acceptor, treating an empty rendered string as the empty-string
/// language (concatenation identity) rather than an invalid regex — [`render_slots`] legitimately
/// returns `""` for an absent/empty pattern (no left-environment declared, an epenthesis-shaped
/// empty LHS, etc.), and `fsm_parse_regex` is not asked to parse that case at all.
fn parse_template(opts: &FomaOptions, text: &str) -> Fsm {
    if text.is_empty() {
        fsm_empty_string()
    } else {
        fsm_parse_regex(opts, text, None, None).unwrap_or_else(|| {
            panic!("pg_foma::lower: foma rejected a lowered span template regex {text:?}")
        })
    }
}

/// Lowers one subrule's `left_env · lhs_focus · right_env` triple (design.md D3's `span(s)`
/// formula) into a pair of foma acceptors over `alphabet`'s token space, for [`spans_overlap`]'s
/// intersection test. `focus` is `RewriteRuleDef.lhs` — shared verbatim across every subrule of
/// one rule (`RewriteSubruleDef` only supplies its own `rhs`/`left_env`/`right_env`, model.rs
/// `RewriteSubruleDef` doc).
///
/// # Why a `(left_language, focus_right_language)` PAIR, not one combined `Fsm`
/// D3 writes `span(s) = left_env · lhs_focus · right_env` and says to intersect two subrules'
/// spans. Read as a literal concatenation of the three patterns' own node sequences and compared
/// as ONE automaton, that is only sound when both subrules' `left_env`/`right_env` describe the
/// SAME fixed length: `left_env`/`right_env` are boundary-anchored templates (they constrain the
/// segments immediately adjacent to the shared focus, not "some point in the word"), so two
/// subrules whose environments describe DIFFERENT lengths (whether because they have different
/// node counts, or — `openspec/changes/compile-bounded-fst-quantifiers` — because one or both
/// contain a bounded `Quantifier` whose own `min..max` range makes even ONE subrule's own template
/// match more than one length) describe overlapping-but-different-length windows around the SAME
/// anchor point. A literal fixed-length concatenation, intersected whole, would (wrongly) report
/// them as non-overlapping merely because the two automata accept different string lengths — an
/// UNSOUND under-refusal (ADR 0001 forbids rounding toward `Admit`; ["`Refuse`(never) rounds toward
/// `Admit`"] is exactly backwards from the required direction). The `Σ*`-padding fix below does not
/// depend on either side being fixed-length in the first place — a bounded quantifier's own
/// template is still a plain REGULAR language (a finite union of finite lengths, exactly what
/// `Slot::Repeat`'s `^{min,max}` compiles to, `crate::replace` module doc's "Bounded quantifiers"),
/// which `fsm_parse_regex` compiles the same as any other template; only a GENUINELY unbounded
/// quantifier stays a [`UnsupportedPatternNode`] here, same as everywhere else in this crate.
///
/// The fix: represent `left_env` as the SUFFIX language `Σ* · left_env` (any prefix, ending in the
/// template) and fold `lhs_focus`/`right_env` into the PREFIX language `lhs_focus · right_env ·
/// Σ*` (starting with the shared focus then the template, any suffix) — each half anchored at the
/// boundary it actually describes, `Σ*` absorbing any length mismatch between the two subrules'
/// own templates. [`spans_overlap`] then intersects the two subrules' LEFT halves and FOCUS+RIGHT
/// halves SEPARATELY (not concatenated into one "contains the whole span somewhere in the word"
/// automaton) — see that function's own doc for why checking them separately is the CORRECT
/// decomposition of D3's "at a shared focus position" requirement, not merely a convenient
/// approximation of one (a single combined `Σ* · L · F · R · Σ*` "contains" automaton would
/// actually be WRONG here: it would accept a witness word where subrule i's context holds at one
/// position and subrule j's holds at an unrelated OTHER position, which is not the same-position
/// overlap D3 asks about).
///
/// # Alpha variables
/// `left_env`/`focus`/`right_env` are lowered with a FRESH, shared occurrence counter local to
/// this call — exactly mirroring how `replace.rs::compile_rewrite_rule_subset` resets
/// `next_occurrence` to `0` per subrule — and jointly resolved via the REUSED
/// [`resolve_alpha_tuples`], so an `AlphaVariable` shared between (say) `left_env` and `focus` is
/// resolved with the SAME joint-agreement semantics real rewrite-rule compilation already uses,
/// not a re-derived one. The subrule's OWN `rhs` is deliberately NOT included in this joint
/// resolution (unlike `replace.rs`'s per-subrule fold, which joins LHS+RHS+left+right together):
/// whether this SPAN can match does not depend on the subrule's RHS at all, and omitting it can
/// only ever ADD spurious alpha tuples relative to the true RHS-constrained set (never remove real
/// ones, since the RHS's own occurrences could only additionally NARROW the joint-agreement
/// filter) — a strictly SAFE, over-permissive simplification that rounds toward more overlap being
/// detected (i.e. toward `Refuse` in [`spans_overlap`]), never an unsound one.
///
/// Each resolved tuple's rendered text is [`fsm_parse_regex`]-compiled (via [`parse_template`])
/// and the per-tuple automata are `fsm_union`-folded per half (a subrule's span matches under ANY
/// of its own valid alpha assignments, not just one) — contrast
/// [`crate::replace::compile_rewrite_rule_subset`]'s per-tuple fold, which is a SEQUENTIAL
/// composition because there each tuple's compiled net is a full elsewhere-preserving REPLACE
/// transducer (that module's own doc: union would reintroduce a spurious "did nothing" path).
/// Here each tuple's compiled net is a plain ACCEPTOR with no "elsewhere" case, so union is exactly
/// the right combinator, not a divergence from that module's reasoning.
///
/// # Returns
/// `Err` names the FIRST unsupported node encountered (checked in `left_env`, `focus`, `right_env`
/// order) via [`UnsupportedPatternNode`] — the caller (`capability.rs`) rounds this to a
/// conservative `Refuse` naming the kind, per this module's own top-doc and design.md D3's "any
/// approximation rounds toward Refuse".
pub fn lower_span(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    left_env: Option<&Pattern>,
    focus: &Pattern,
    right_env: Option<&Pattern>,
) -> Result<(Fsm, Fsm), UnsupportedPatternNode> {
    let mut next_occurrence = 0usize;
    // `openspec/changes/fix-multitable-fst-compilation`: `pattern_slots`/`resolve_alpha_tuples`
    // now take an explicit table (no more implicit `g.char_tables[0]`) -- `alphabet.table()` is
    // already the correct table for this call, by this function's OWN caller contract (module
    // doc: `lower_span` is handed whichever `SegAlphabet` its caller already resolved correctly,
    // e.g. `capability.rs`'s `lower_subrule_span` now resolves it via
    // `crate::replace::owning_table` too -- see that function's own doc).
    let table = alphabet.table();

    // `PatternLowerScope::Baseline`: `lower_span` is `SimultaneousSubruleOverlapPredicate`'s own
    // machinery (D3's `hc.dll`-oracle-verified span-intersection test, module top doc) -- it MUST
    // stay on this tier permanently, unaffected by task 4.2's `RewriteRuleCompile` widening
    // elsewhere in this module (`PatternLowerScope`'s own doc).
    let scope = PatternLowerScope::Baseline;
    let left_slots = match left_env {
        Some(p) => pattern_slots(g, table, p, &mut next_occurrence, scope)
            .ok_or_else(|| diagnose_unsupported(g, table, p, scope))?,
        None => Vec::new(),
    };
    let focus_slots = pattern_slots(g, table, focus, &mut next_occurrence, scope)
        .ok_or_else(|| diagnose_unsupported(g, table, focus, scope))?;
    let right_slots = match right_env {
        Some(p) => pattern_slots(g, table, p, &mut next_occurrence, scope)
            .ok_or_else(|| diagnose_unsupported(g, table, p, scope))?,
        None => Vec::new(),
    };

    let (assignments, _report) = resolve_alpha_tuples(
        table,
        &[
            left_slots.as_slice(),
            focus_slots.as_slice(),
            right_slots.as_slice(),
        ],
    );

    let mut left_lang: Option<Fsm> = None;
    let mut focus_right_lang: Option<Fsm> = None;
    for asg in &assignments {
        let left_text = render_slots(alphabet, &left_slots, asg);
        let focus_text = render_slots(alphabet, &focus_slots, asg);
        let right_text = render_slots(alphabet, &right_slots, asg);

        let left_tpl = parse_template(opts, &left_text);
        let focus_tpl = parse_template(opts, &focus_text);
        let right_tpl = parse_template(opts, &right_text);

        // Sigma* . left_template  (suffix language: any prefix, ending in the left template).
        let this_left = fsm_concat(opts, fsm_universal(), left_tpl);
        // focus_template . right_template . Sigma*  (prefix language: starts with the shared
        // focus then the right template, any suffix).
        let this_focus_right = fsm_concat(
            opts,
            fsm_concat(opts, focus_tpl, right_tpl),
            fsm_universal(),
        );

        left_lang = Some(match left_lang {
            None => this_left,
            Some(prev) => fsm_union(opts, prev, this_left),
        });
        focus_right_lang = Some(match focus_right_lang {
            None => this_focus_right,
            Some(prev) => fsm_union(opts, prev, this_focus_right),
        });
    }

    // `assignments` is empty only when the joint-agreement filter finds NO valid alpha tuple at
    // all (a subrule whose own environment/focus alpha constraints are mutually unsatisfiable) --
    // the empty language is the exactly-correct span for a subrule that can never match anything.
    Ok((
        left_lang.unwrap_or_else(fsm_empty_set),
        focus_right_lang.unwrap_or_else(fsm_empty_set),
    ))
}

/// design.md D3's intersection test: `true` iff subrules `a` and `b`'s spans (each a
/// `(left_language, focus_right_language)` pair from [`lower_span`]) can hold AT THE SAME shared
/// focus position — i.e. genuinely overlap.
///
/// # Why two independent intersections, not one combined automaton
/// The real actual-word content immediately LEFT of the shared focus position is ONE concrete
/// (finite) string; it satisfies subrule `a`'s left environment iff it is a member of `a`'s
/// `left_language`, and independently satisfies `b`'s iff it is a member of `b`'s `left_language`
/// — both languages describe THE SAME region of the SAME word, so the question "can some real
/// left-context simultaneously satisfy both" is exactly `intersect(left_a, left_b)` non-empty, no
/// further alignment machinery needed (the `Σ*` prefix in each already anchors the comparison at
/// the shared right edge — see [`lower_span`]'s own doc). The symmetric argument holds for the
/// content AT/RIGHT of the position via `focus_right_language`. Because the left region and the
/// focus+right region of a word are DISJOINT and freely composable (any accepted left-string
/// concatenated with any accepted focus+right-string is a valid witness word — nothing else
/// constrains them jointly once each subrule's OWN internal alpha agreement has already been
/// resolved inside [`lower_span`]), `a` and `b` can co-fire at the same position iff BOTH
/// intersections are non-empty; checking them as one combined `Σ* · L · F · R · Σ*` "contains
/// somewhere" automaton instead would be WRONG (see [`lower_span`]'s own doc for the false-overlap
/// case that construction admits).
///
/// Any imprecision [`lower_span`]'s per-subrule marginalization introduces (projecting each of a
/// subrule's OWN internally-consistent alpha tuples down to a left-only / focus+right-only piece
/// before unioning across tuples) can only ever make a language LARGER than the true "matches
/// under some single self-consistent assignment" set — i.e. can only report MORE overlap than
/// truly exists, never less — which rounds toward `Refuse`, the safe direction (ADR 0001).
pub fn spans_overlap(opts: &FomaOptions, a: &(Fsm, Fsm), b: &(Fsm, Fsm)) -> bool {
    let (left_a, focus_right_a) = a;
    let (left_b, focus_right_b) = b;

    let mut left_intersection = fsm_intersect(opts, left_a.clone(), left_b.clone());
    if fsm_isempty(opts, &mut left_intersection) {
        return false;
    }
    let mut focus_right_intersection =
        fsm_intersect(opts, focus_right_a.clone(), focus_right_b.clone());
    !fsm_isempty(opts, &mut focus_right_intersection)
}

#[cfg(test)]
mod tests {
    //! Synthetic, delanguaged fixtures only (no natural-language names), mirroring
    //! `capability.rs`'s own test-module convention.

    use pg_grammar::model::{PhonRuleDef, RewriteMode};

    use super::*;

    fn load(xml: &str) -> pg_grammar::model::Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    const OVERLAP_LOWER_PROBE_XML: &str = r#"<HermitCrabInput><Language><Name>OverlapLowerProbe</Name>
      <PhonologicalFeatureSystem>
        <SymbolicFeature id="featPlace"><Name>place</Name>
          <Symbols>
            <Symbol id="symNeutral">neutral</Symbol>
            <Symbol id="symFront">front</Symbol>
            <Symbol id="symBack">back</Symbol>
          </Symbols>
        </SymbolicFeature>
      </PhonologicalFeatureSystem>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="cStop"><Representations><Representation>p</Representation></Representations>
            <FeatureValue feature="featPlace" symbolValues="symNeutral" />
          </SegmentDefinition>
          <SegmentDefinition id="cFront"><Representations><Representation>i</Representation></Representations>
            <FeatureValue feature="featPlace" symbolValues="symFront" />
          </SegmentDefinition>
          <SegmentDefinition id="cBack"><Representations><Representation>u</Representation></Representations>
            <FeatureValue feature="featPlace" symbolValues="symBack" />
          </SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses>
        <SegmentNaturalClass id="ncStop"><Name>Stop</Name><Segment segment="cStop" /></SegmentNaturalClass>
        <FeatureNaturalClass id="ncFront"><Name>Front</Name>
          <FeatureValue feature="featPlace" symbolValues="symFront" />
        </FeatureNaturalClass>
        <FeatureNaturalClass id="ncBack"><Name>Back</Name>
          <FeatureValue feature="featPlace" symbolValues="symBack" />
        </FeatureNaturalClass>
      </NaturalClasses>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prNoOverlap" multipleApplicationOrder="simultaneous"><Name>noOverlap</Name>
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
        <PhonologicalRule id="prOverlap" multipleApplicationOrder="simultaneous"><Name>overlap</Name>
          <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
    </Language></HermitCrabInput>"#;

    fn rewrite_rule<'g>(
        g: &'g pg_grammar::model::Grammar,
        xml_id: &str,
    ) -> &'g pg_grammar::model::RewriteRuleDef {
        for pr in &g.prules {
            if let PhonRuleDef::Rewrite(r) = pr {
                if r.xml_id == xml_id {
                    return r;
                }
            }
        }
        panic!("rewrite rule {xml_id:?} not found");
    }

    /// Two subrules whose RIGHT environments are mutually exclusive natural classes (`Front` vs.
    /// `Back`, no overlapping segments) must lower to spans whose focus+right intersection is
    /// EMPTY -- they cannot both hold at the same position.
    #[test]
    fn lower_span_disjoint_right_environments_do_not_overlap() {
        let g = load(OVERLAP_LOWER_PROBE_XML);
        let r = rewrite_rule(&g, "prNoOverlap");
        assert_eq!(r.mode, RewriteMode::Simultaneous);
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();

        let span_a = lower_span(
            &opts,
            &g,
            &alphabet,
            r.subrules[0].left_env.as_ref(),
            &r.lhs,
            r.subrules[0].right_env.as_ref(),
        )
        .expect("prNoOverlap subrule 0 must lower (no unsupported nodes)");
        let span_b = lower_span(
            &opts,
            &g,
            &alphabet,
            r.subrules[1].left_env.as_ref(),
            &r.lhs,
            r.subrules[1].right_env.as_ref(),
        )
        .expect("prNoOverlap subrule 1 must lower (no unsupported nodes)");

        assert!(
            !spans_overlap(&opts, &span_a, &span_b),
            "Front/Back-flanked subrules must NOT overlap"
        );
    }

    /// Two subrules with IDENTICAL (unconstrained) focus/environment lower to the SAME span --
    /// their intersection is trivially non-empty.
    #[test]
    fn lower_span_identical_unconstrained_subrules_overlap() {
        let g = load(OVERLAP_LOWER_PROBE_XML);
        let r = rewrite_rule(&g, "prOverlap");
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();

        let span_a = lower_span(
            &opts,
            &g,
            &alphabet,
            r.subrules[0].left_env.as_ref(),
            &r.lhs,
            r.subrules[0].right_env.as_ref(),
        )
        .expect("prOverlap subrule 0 must lower");
        let span_b = lower_span(
            &opts,
            &g,
            &alphabet,
            r.subrules[1].left_env.as_ref(),
            &r.lhs,
            r.subrules[1].right_env.as_ref(),
        )
        .expect("prOverlap subrule 1 must lower");

        assert!(
            spans_overlap(&opts, &span_a, &span_b),
            "two unconstrained same-focus subrules must overlap"
        );
    }

    // =============================================================================================
    // `openspec/changes/build-unbounded-quantifier-support` (tasks.md 4.5): `Slot::Repeat.max`
    // widened to `Option<u32>` -- a genuinely unbounded (`max: None`) quantifier now compiles via
    // foma's own native `E*`/`E^>N` operator, `MAX_QUANTIFIER_BOUND`/the inverted-bound check apply
    // ONLY to a finite `max`, and every OTHER out-of-scope shape (inverted finite, over-budget
    // finite, alpha-nested) stays exactly as unsupported as before.
    // =============================================================================================

    use foma::apply::{apply_init, apply_up};

    /// One `<CharacterDefinitionTable>` (a single segment `c1`) and five `<PhonologicalRule>`s, each
    /// a bare `Segment`-focused LHS with ONE quantifier-bearing probe -- no `Environment`/`RHS`
    /// content is load-bearing here (these fixtures are never compiled/composed, only fed straight
    /// to [`pattern_slots`] via each rule's own `lhs`), mirroring `OVERLAP_LOWER_PROBE_XML`'s own
    /// "cheap structural probe, not an end-to-end compile" convention.
    const QUANTIFIER_SCOPE_PROBE_XML: &str = r#"<HermitCrabInput><Language><Name>QuantifierScopeProbe</Name>
      <PhonologicalFeatureSystem>
        <SymbolicFeature id="featA"><Name>a</Name>
          <Symbols><Symbol id="symX">x</Symbol><Symbol id="symY">y</Symbol></Symbols>
        </SymbolicFeature>
      </PhonologicalFeatureSystem>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="c1"><Representations><Representation>a</Representation></Representations>
            <FeatureValue feature="featA" symbolValues="symX" />
          </SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses>
        <SegmentNaturalClass id="ncC1"><Name>C1</Name><Segment segment="c1" /></SegmentNaturalClass>
      </NaturalClasses>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prUnboundedMinZero"><Name>demo0</Name>
          <PhoneticInput><PhoneticSequence>
            <OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncC1" /></OptionalSegmentSequence>
          </PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prUnboundedLargeMin"><Name>demo1</Name>
          <PhoneticInput><PhoneticSequence>
            <OptionalSegmentSequence min="1000" max="-1"><SimpleContext naturalClass="ncC1" /></OptionalSegmentSequence>
          </PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prInvertedFinite"><Name>demo2</Name>
          <PhoneticInput><PhoneticSequence>
            <OptionalSegmentSequence min="5" max="2"><SimpleContext naturalClass="ncC1" /></OptionalSegmentSequence>
          </PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prOverBudgetFinite"><Name>demo3</Name>
          <PhoneticInput><PhoneticSequence>
            <OptionalSegmentSequence min="1" max="600"><SimpleContext naturalClass="ncC1" /></OptionalSegmentSequence>
          </PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prAlphaNestedUnbounded"><Name>demo4</Name>
          <VariableFeatures><VariableFeature id="var1" name="a" phonologicalFeature="featA" /></VariableFeatures>
          <PhoneticInput><PhoneticSequence>
            <OptionalSegmentSequence min="1" max="-1">
              <SimpleContext naturalClass="ncC1"><AlphaVariables><AlphaVariable variableFeature="var1" /></AlphaVariables></SimpleContext>
            </OptionalSegmentSequence>
          </PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
    </Language></HermitCrabInput>"#;

    fn quantifier_probe_rule<'g>(
        g: &'g pg_grammar::model::Grammar,
        xml_id: &str,
    ) -> &'g pg_grammar::model::RewriteRuleDef {
        for pr in &g.prules {
            if let PhonRuleDef::Rewrite(r) = pr {
                if r.xml_id == xml_id {
                    return r;
                }
            }
        }
        panic!("rule {xml_id:?} not found");
    }

    /// Positive witness: a genuinely unbounded (`max="-1"`), `min="0"` quantifier now lowers to
    /// `Some(_)` (a `Slot::Repeat { min: 0, max: None, .. }`) -- it used to be an unconditional
    /// `None` regardless of `min`/`max` (module doc's ORIGINAL scope line).
    #[test]
    fn unbounded_quantifier_min_zero_is_accepted_not_refused() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let rule = quantifier_probe_rule(&g, "prUnboundedMinZero");
        let mut next_occurrence = 0usize;
        let slots = pattern_slots(
            &g,
            table,
            &rule.lhs,
            &mut next_occurrence,
            PatternLowerScope::Baseline,
        )
        .expect("a well-formed unbounded (min=0), alpha-free quantifier must now lower");
        assert_eq!(slots.len(), 1);
        match &slots[0] {
            Slot::Repeat { min, max, .. } => {
                assert_eq!(*min, 0);
                assert_eq!(*max, None);
            }
            _ => panic!("expected a Slot::Repeat"),
        }
    }

    /// Positive witness: [`MAX_QUANTIFIER_BOUND`] (512) is NEVER checked against an unbounded
    /// quantifier's own `min` -- `min=1000` (well past 512) must still lower to `Some(_)`, proving
    /// the ceiling is skipped entirely for `max: None`, not merely "not tripped by coincidence".
    #[test]
    fn unbounded_quantifier_large_min_is_never_checked_against_max_quantifier_bound() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let rule = quantifier_probe_rule(&g, "prUnboundedLargeMin");
        let mut next_occurrence = 0usize;
        let slots = pattern_slots(&g, table, &rule.lhs, &mut next_occurrence, PatternLowerScope::Baseline).expect(
            "min=1000 (> MAX_QUANTIFIER_BOUND=512) must NOT be refused for an unbounded (max=None) \
             quantifier -- that ceiling only bounds a FINITE max, never `None`",
        );
        match &slots[0] {
            Slot::Repeat { min, max, .. } => {
                assert_eq!(*min, 1000);
                assert_eq!(*max, None);
            }
            _ => panic!("expected a Slot::Repeat"),
        }
    }

    /// Negative witness: an inverted FINITE bound (`min=5 > max=2`, both concrete) has no sound
    /// finite construction and must stay refused, unaffected by the unbounded-quantifier widening.
    #[test]
    fn inverted_finite_quantifier_still_unsupported() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let rule = quantifier_probe_rule(&g, "prInvertedFinite");
        let mut next_occurrence = 0usize;
        assert!(
            pattern_slots(
                &g,
                table,
                &rule.lhs,
                &mut next_occurrence,
                PatternLowerScope::Baseline
            )
            .is_none(),
            "min=5 > max=2 (both concrete) must stay refused"
        );
    }

    /// Negative witness: a FINITE `max` past [`MAX_QUANTIFIER_BOUND`] (512) must stay refused, never
    /// silently clamped down to the ceiling -- unaffected by the unbounded-quantifier widening
    /// (which only ever ADDS a new accepted shape, `max: None`; it never loosens the finite check).
    #[test]
    fn over_budget_finite_quantifier_still_unsupported() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let rule = quantifier_probe_rule(&g, "prOverBudgetFinite");
        let mut next_occurrence = 0usize;
        assert!(
            pattern_slots(
                &g,
                table,
                &rule.lhs,
                &mut next_occurrence,
                PatternLowerScope::Baseline
            )
            .is_none(),
            "max=600 exceeds MAX_QUANTIFIER_BOUND=512 -- must stay refused, never silently clamped"
        );
    }

    /// Negative witness: an `AlphaVariable` occurrence inside a quantifier's own children is out of
    /// scope regardless of whether the quantifier itself is bounded or unbounded ([`Slot::Repeat`]'s
    /// own doc) -- `max="-1"` here does not change that.
    #[test]
    fn alpha_nested_unbounded_quantifier_still_unsupported() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let rule = quantifier_probe_rule(&g, "prAlphaNestedUnbounded");
        let mut next_occurrence = 0usize;
        assert!(
            pattern_slots(
                &g,
                table,
                &rule.lhs,
                &mut next_occurrence,
                PatternLowerScope::Baseline
            )
            .is_none(),
            "an AlphaVariable occurrence inside a quantifier's own children is out of scope \
             regardless of whether the quantifier itself is bounded or unbounded"
        );
    }

    /// **Load-bearing off-by-one, pinned at the COMPILED FST level, not just the rendered text.**
    /// Foma's own `E^>N` xre operator means "MORE THAN `N`", i.e. `N+1` or more (`nfst-xre`'s
    /// `RepeatNPlus`, `foma-0.4.2/src/regex.rs:258-268`'s own `concat(concat_n(net, N),
    /// kleene_plus(net))`) -- so rendering "`min` or more" needs `^>(min-1)`, never `^>min`. `min=2`
    /// must render `^>1` and its compiled net must accept exactly 2 (and 3+) occurrences while
    /// REJECTING 1 -- if `render_slots` instead emitted `^>min` (`^>2`), exactly 2 occurrences would
    /// wrongly fail to match (this test would catch that regression).
    #[test]
    fn render_slots_unbounded_min_off_by_one_boundary() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();
        let (cd, _) = table
            .iter()
            .next()
            .expect("QUANTIFIER_SCOPE_PROBE_XML's table must have exactly 1 segment");
        let tok = alphabet.token(cd).to_string();

        let slots = vec![Slot::Repeat {
            min: 2,
            max: None,
            children: vec![Slot::Fixed(cd)],
        }];
        let asg = AlphaAssignment {
            values: std::collections::HashMap::new(),
        };
        let text = render_slots(&alphabet, &slots, &asg);
        assert_eq!(
            text,
            format!("[{tok}]^>1"),
            "min=2 (\"2 or more\") must render as ^>1 (min-1), never ^>2 (min)"
        );

        let net = fsm_parse_regex(&opts, &text, None, None)
            .expect("rendered unbounded-quantifier text must compile");
        let one = tok.clone();
        let two = format!("{tok}{tok}");
        let three = format!("{tok}{tok}{tok}");

        let mut h = apply_init(&net);
        assert_eq!(
            apply_up(&mut h, Some(&one)),
            None,
            "1 occurrence (below min=2) must NOT match"
        );
        let mut h = apply_init(&net);
        assert_eq!(
            apply_up(&mut h, Some(&two)),
            Some(two.clone()),
            "exactly min=2 occurrences must match -- the off-by-one this test pins"
        );
        let mut h = apply_init(&net);
        assert_eq!(
            apply_up(&mut h, Some(&three)),
            Some(three.clone()),
            "MORE than min (3) must ALSO match -- genuinely unbounded, not a min..min+1 accident"
        );
    }

    /// `min == 0` ("zero or more") renders as plain `*` (foma's native Kleene star), matching zero,
    /// one, or many occurrences -- distinct code path from the `min >= 1` `^>` case above.
    #[test]
    fn render_slots_unbounded_min_zero_is_kleene_star() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();
        let (cd, _) = table
            .iter()
            .next()
            .expect("QUANTIFIER_SCOPE_PROBE_XML's table must have exactly 1 segment");
        let tok = alphabet.token(cd).to_string();

        let slots = vec![Slot::Repeat {
            min: 0,
            max: None,
            children: vec![Slot::Fixed(cd)],
        }];
        let asg = AlphaAssignment {
            values: std::collections::HashMap::new(),
        };
        let text = render_slots(&alphabet, &slots, &asg);
        assert_eq!(
            text,
            format!("[{tok}]*"),
            "min=0 or more must render as a plain Kleene star"
        );

        let net = fsm_parse_regex(&opts, &text, None, None)
            .expect("rendered unbounded-quantifier text must compile");
        let zero = String::new();
        let one = tok.clone();
        let two = format!("{tok}{tok}");

        let mut h = apply_init(&net);
        assert_eq!(
            apply_up(&mut h, Some(&zero)),
            Some(zero.clone()),
            "0 occurrences must match"
        );
        let mut h = apply_init(&net);
        assert_eq!(
            apply_up(&mut h, Some(&one)),
            Some(one.clone()),
            "1 occurrence must match"
        );
        let mut h = apply_init(&net);
        assert_eq!(
            apply_up(&mut h, Some(&two)),
            Some(two.clone()),
            "2 occurrences must match"
        );
    }

    /// Cross-table representation aliasing (`docs/conformance/multitable-shared-representation-
    /// design.md` item 3): `render_slots`' `Slot::Fixed`/`Slot::Union` arms, not `class_members`.
    /// Two tables (t0: one segment "x"; t1: "z"/"x"/"y", "x" deliberately at a DIFFERENT raw index
    /// than t0's own) -- an alphabet built via `SegAlphabet::with_table_id(table_b, ..)` must
    /// render t1's own "x" atom as the bracketed union of BOTH tables' tokens, while `SegAlphabet::
    /// new` (no table identity, every pre-existing call site) renders the SAME slot bare, byte-
    /// identical to pre-aliasing behavior.
    #[test]
    fn render_slots_aliases_fixed_and_union_atoms_across_tables() {
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>RenderSlotsAliasProbe</Name>
  <CharacterDefinitionTable id="t0"><Name>TableA</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="c0x"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <CharacterDefinitionTable id="t1"><Name>TableB</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="c1z"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="c1x"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="c1y"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
</Language></HermitCrabInput>"#;
        let g = load(XML);
        let table_a = &g.char_tables[0];
        let table_b = &g.char_tables[1];
        let cd_a_x = table_a.lookup_nfd("x").unwrap();
        let cd_b_x = table_b.lookup_nfd("x").unwrap();
        let cd_b_y = table_b.lookup_nfd("y").unwrap();
        assert_ne!(
            cd_a_x.0, cd_b_x.0,
            "the fixture's own misalignment must hold"
        );

        let alias_map = crate::replace::RepresentationAliasMap::build(&g);
        let aliased =
            SegAlphabet::with_table_id(table_b, pg_grammar::model::TableId(1), &alias_map);
        let bare = SegAlphabet::new(table_b);
        let asg = AlphaAssignment {
            values: std::collections::HashMap::new(),
        };

        // Slot::Fixed: the shared "x" atom aliases under `aliased`, stays bare under `bare`.
        let fixed_slots = vec![Slot::Fixed(cd_b_x)];
        let aliased_text = render_slots(&aliased, &fixed_slots, &asg);
        let bare_text = render_slots(&bare, &fixed_slots, &asg);
        assert_eq!(
            bare_text,
            bare.token(cd_b_x).to_string(),
            "unaliased rendering must stay exactly the single bare token"
        );
        assert_ne!(
            aliased_text, bare_text,
            "aliased rendering of a shared atom must differ from the unaliased rendering"
        );
        assert!(
            aliased_text.contains(&bare.token(cd_b_x).to_string())
                && aliased_text.contains(&SegAlphabet::new(table_a).token(cd_a_x).to_string()),
            "aliased rendering must contain BOTH tables' own tokens for the shared spelling: \
             {aliased_text:?}"
        );

        // Slot::Union: an unshared atom ("y") degenerates to the SAME bare rendering as `bare`,
        // aliased or not -- aliasing only ever adds, never touches a spelling unique to its table.
        let union_slots = vec![Slot::Union(vec![cd_b_y])];
        assert_eq!(
            render_slots(&aliased, &union_slots, &asg),
            render_slots(&bare, &union_slots, &asg),
            "an unshared atom's rendering must be unaffected by aliasing"
        );
    }
}
