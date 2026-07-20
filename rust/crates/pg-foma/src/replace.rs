//! P6 feasibility prototype (docs/fst-plan/foma-fst-plan.md §P6 item 1, `docs/fst-plan/
//! p6-prototype-report.md`): compile HC [`RewriteRuleDef`]s into real foma replace-calculus
//! regex source (`A -> B || L _ R`), instead of enumerating every surface junction variant at
//! build time the way [`crate::junctions`]/[`crate::preexpand`] do. This module is NOT wired into
//! the mainline `emit`/`analyzer` path — it is a standalone prototype module exercised by
//! `examples/p6_replace_prototype.rs`.
//!
//! ## Symbol alphabet: char-def IDENTITY, not literal spelling
//! The engine matches phonological segments by **char-def identity**, never by literal spelling
//! (`emit.rs`'s module doc, "Surface spelling": a char-def with several `<Representation>`s
//! matches ANY of its own spellings). `emit.rs` copes with this by cartesian-producting every
//! spelling variant into literal lexc strings ([`crate::emit::surface_variants`]). This module
//! takes the more direct route available once lexc/rules are built from [`pg_shape::Shape`]
//! structure rather than raw text: every [`CharDefId`] used anywhere in the grammar's surface
//! table is mapped to **one Private-Use-Area codepoint** ([`SegAlphabet::token`]), and every lexc
//! entry, rule regex, and query word is built/encoded in that token space. This sidesteps BOTH
//! footguns literal-string lexc has to work around:
//! - multi-representation segments (Indonesian's `char28` = {"g","G"}) need no cartesian product
//!   at all — both spellings segment to the SAME char-def id, hence the SAME token, for free;
//! - multi-character graphemes ("ng"/"ny"/"sy"/"kh") need no lexc `Multichar_Symbols`
//!   declaration/registration bookkeeping between the lexc compile and the (separately compiled,
//!   then composed) rule regexes — each grapheme is already one token, one codepoint, matched by
//!   plain regex concatenation.
//! - xre-reserved characters (the morpheme-boundary `+` is foma's Kleene-plus operator!) never
//!   collide with a token, since PUA codepoints are outside xre's entirely-ASCII reserved set.
//!
//! The price: the composed network's own lower tape is not human-legible orthography. That's
//! fine for the propose→confirm contract (plan §1): [`FomaProposer`]-equivalent callers only need
//! the UPPER tape's tag sequence; a query word is transliterated into token space
//! ([`SegAlphabet::encode_query`], reusing [`pg_grammar::segment::segment_phonemes_only`] — the
//! same greedy longest-match the engine's own segmentation uses) before `apply_up`, and the
//! result is decoded via [`crate::tags::decode_path`] exactly like the mainline proposer.
//!
//! ## alpha-variable expansion: tuple-indexed, not per-variable (reports/08 §3.1)
//! A rule's alpha-bound slots (RHS/LHS/environment [`PatternNode::Context`] nodes carrying
//! [`AlphaVar`]s) are resolved by [`resolve_alpha_tuples`]: gather every slot referencing a given
//! [`VarId`], enumerate the CROSS PRODUCT of each slot's own (non-alpha-feature) candidate
//! members, then keep only the combinations where every pair of same-`VarId` slots agrees (same
//! symbolic-feature value at that variable's lane — `AlphaVar::plus` polarity; `minus`/"disagree"
//! is unimplemented, see the doc on [`AlphaOccurrence`]). This is the "count of segment tuples
//! satisfying the joint constraint" bound from `reports/08-audit-corrections-and-reframed-
//! architecture.md` §3 item 1 (Amharic's 20-variable CV-merger: nc15=59 × nc16=6 ⇒ ≤354, never
//! v^20) — implemented once, generically over N variables and N slots-per-variable, so the same
//! code path that resolves Indonesian's single-variable prule4 is what would resolve Amharic's
//! rule without modification.
//!
//! ## What this module does NOT attempt (see the prototype report for the full list)
//! - [`PatternNode::Quantifier`] (`OptionalSegmentSequence`, prule3's own left-environment) —
//!   [`pattern_slots`] returns `None` or bails when it meets one; a rule whose pattern needs it is
//!   reported uncovered, not silently mis-rendered.
//! - [`AlphaVar::plus`] == `false` ("disagree" polarity) — no reference-grammar rule needs it.
//! - `RewriteMode::Simultaneous` vs `Iterative` distinction, and `Dir::RightToLeft` — Indonesian's
//!   5 rules are all `Iterative`/`LeftToRight`; every subrule is compiled with plain foma `->`
//!   (see the report for the mapping-fidelity discussion; the `foma` crate's `src/reverse.rs`'s
//!   `fsm_reverse` is the standard primitive `RightToLeft` would need, unexercised here).
//! - MPR gating (`required_mpr`/`excluded_mpr` on a subrule) — flag-diacritic emission is P6
//!   mainline work per the plan (`§P6` item 1's own text), not attempted in this slice.

use std::collections::HashSet;

use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;

use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::{
    Dir, Grammar, NaturalClassKind, Pattern, PatternNode, PhonRuleDef, RewriteMode,
    RewriteRuleDef, RewriteSubruleDef, VarId,
};

use crate::compose_budget::{compose_checked, ComposeBudget, ComposeError};

/// Private-Use-Area base codepoint every [`CharDefId`] is offset from. `CharDefId`s in every
/// reference grammar are far below `0xF8FF - 0xE000` (6400) entries, so no grammar in scope can
/// overflow the PUA block.
const PUA_BASE: u32 = 0xE000;

/// Maps [`CharDefId`]s to/from single Private-Use-Area codepoints (module doc). Cheap to
/// construct (`table` borrow only); one instance is shared across rule compilation, lexc
/// emission, and query encoding for one grammar/table pair.
pub struct SegAlphabet<'t> {
    table: &'t CharDefTable,
}

impl<'t> SegAlphabet<'t> {
    pub fn new(table: &'t CharDefTable) -> Self {
        SegAlphabet { table }
    }

    /// The single codepoint standing in for `cd` everywhere (lexc lower-tape text, rule regex
    /// atoms, encoded query words).
    pub fn token(&self, cd: CharDefId) -> char {
        char::from_u32(PUA_BASE + cd.0).expect("char table too large for the PUA token scheme")
    }

    /// Encode a [`pg_shape::Shape`]'s interior nodes (module doc's "already-segmented" shortcut —
    /// root/affix authored text is segmented once at grammar load; this just replays that Shape,
    /// never re-parsing the text) into one token string, Segment and Boundary nodes both kept (a
    /// rule's own context can reference either kind — Indonesian's boundary `char30` is itself
    /// just another char-def with its own token here, module doc).
    pub fn encode_shape(&self, shape: &pg_shape::Shape) -> String {
        shape
            .interior()
            .map(|(_, _, cd, _)| self.token(CharDefId(cd)))
            .collect()
    }

    /// Transliterate a real orthographic query word into token space via
    /// `pg_grammar::segment::segment_phonemes_only` (drop boundaries — a real surface word never
    /// contains a literal morpheme-boundary character). `None` if the word fails to segment
    /// against this grammar's own surface table (same failure mode `emit.rs`'s query path has).
    pub fn encode_query(&self, word: &str) -> Option<String> {
        let shape = pg_grammar::segment::segment_phonemes_only(self.table, word).ok()?;
        Some(
            shape
                .interior()
                .map(|(_, _, cd, _)| self.token(CharDefId(cd)))
                .collect(),
        )
    }

    pub fn table(&self) -> &'t CharDefTable {
        self.table
    }
}

// =================================================================================================
// Natural-class member resolution (exact, from the model's own `NaturalClassKind` — never
// re-derived through a matcher-oriented helper whose semantics are tuned for a different job).
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

/// One position in a rendered pattern.
enum Slot {
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
}

/// Walk `pattern`'s nodes into [`Slot`]s, numbering each `Alpha` occurrence sequentially from
/// `*next_occurrence` (shared across LHS/RHS/left-env/right-env for one subrule — see
/// [`compile_rewrite_rule`]). Returns `None` (uncovered) on `Quantifier`/`Anchor` — this
/// prototype's documented scope line (module doc).
fn pattern_slots(g: &Grammar, pattern: &Pattern, next_occurrence: &mut usize) -> Option<Vec<Slot>> {
    let mut out = Vec::with_capacity(pattern.nodes.len());
    for node in &pattern.nodes {
        match node {
            PatternNode::CharDef(id) => out.push(Slot::Fixed(*id)),
            PatternNode::Context(sc) => {
                if sc.vars.is_empty() {
                    let members = class_members(g, table_of(g, sc), sc.nat_class, &HashSet::new());
                    out.push(Slot::Union(members));
                } else {
                    if sc.vars.iter().any(|v| !v.plus) {
                        // "disagree" polarity — documented gap, never seen in the reference
                        // grammars (module doc).
                        return None;
                    }
                    let excl: HashSet<usize> =
                        sc.vars.iter().map(|v| v.feature.0 as usize).collect();
                    let base = class_members(g, table_of(g, sc), sc.nat_class, &excl);
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
            PatternNode::Quantifier { .. } | PatternNode::Segments { .. } | PatternNode::Anchor(_) => {
                return None;
            }
        }
    }
    Some(out)
}

/// Every `Context` node in a `Pattern` carries `NatClassId`; resolving its table is a matter of
/// which stratum owns the rule, but since `Grammar` doesn't expose a per-class table pointer,
/// this prototype resolves natural classes against the SAME table every rule/lexicon in a
/// single-table grammar uses (true for Indonesian — one `<CharacterDefinitionTable>`). A
/// multi-table grammar would need the owning stratum threaded through; documented as a mainline
/// gap (this prototype targets Indonesian, one table).
fn table_of<'g>(g: &'g Grammar, _sc: &pg_grammar::model::SimpleContext) -> &'g CharDefTable {
    &g.char_tables[0]
}

// =================================================================================================
// Alpha-tuple resolution (reports/08 §3.1): cartesian product per variable, filtered by joint
// agreement, generic over N variables / N slots-per-variable.
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
fn resolve_alpha_tuples(g: &Grammar, slot_lists: &[&[Slot]]) -> (Vec<AlphaAssignment>, TupleReport) {
    let table = &g.char_tables[0];
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
    let mut var_pairs: std::collections::HashMap<VarId, Vec<(usize, pg_grammar::featsys::FlatIndex)>> =
        std::collections::HashMap::new();
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
// =================================================================================================

/// Renders `slots` to xre source text, ONE SPACE between consecutive slots (never omitted — see
/// the module-level "foma-rs API findings" note below on why). A single space also separates
/// union members inside one `[...]` group, same reason.
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
/// symbols, which is exactly what a char-def-identity token alphabet (module doc) is built from.
/// Mainline P6 must carry this forward as a hard rule for ANY xre string this compiler emits.
fn render_slots(alphabet: &SegAlphabet, slots: &[Slot], assignment: &AlphaAssignment) -> String {
    let mut pieces: Vec<String> = Vec::with_capacity(slots.len());
    for slot in slots {
        let piece = match slot {
            Slot::Fixed(cd) => alphabet.token(*cd).to_string(),
            Slot::Union(members) => {
                if members.len() == 1 {
                    alphabet.token(members[0]).to_string()
                } else {
                    let inner: Vec<String> =
                        members.iter().map(|m| alphabet.token(*m).to_string()).collect();
                    format!("[{}]", inner.join(" | "))
                }
            }
            Slot::Alpha { occurrence, .. } => {
                let cd = assignment.values.get(occurrence).expect(
                    "every alpha slot's occurrence has a resolved assignment by render time",
                );
                alphabet.token(*cd).to_string()
            }
        };
        pieces.push(piece);
    }
    pieces.join(" ")
}

// =================================================================================================
// One subrule -> one or more concrete xre replace-rule instances, COMPOSED (not unioned!) at the
// Fsm level. Two false starts, both empirically confirmed and worth recording (prototype
// report's "foma-rs API findings"):
//   1. Comma-joining full `LHS -> RHS || L _ R` branches in ONE regex string is rejected by this
//      vendored xre grammar's parser whenever the branches don't share one RHS (this foma-rs's
//      comma only joins multiple ENVIRONMENTS for a SHARED `LHS -> RHS`, or fully bare
//      `LHS -> RHS` rules with no `||` at all — confirmed by direct bisection).
//   2. `fsm_union`-folding N independently-compiled per-tuple nets is WRONG, not just
//      syntactically awkward: each per-tuple net is a COMPLETE replace transducer whose own
//      "elsewhere" case is already identity (everything outside ITS OWN context passes through
//      unchanged). Unioning N such complete nets reintroduces a SPURIOUS "did nothing" path at
//      every position — including ones where some OTHER tuple's context obligatorily applies —
//      because from any ONE tuple's-net point of view, a position outside its own context is
//      just ordinary passthrough, and union keeps that path alongside the correct one. Verified
//      empirically: `apply_down` on a hand-built "meⁿ+baca" underlying string through the union
//      returned BOTH the correct "mem+baca" path AND a spurious "meⁿ+baca" (unconverted
//      placeholder) path.
// The fix: since the 14 tuples' contexts are, BY THE JOINT-AGREEMENT FILTER'S OWN CONSTRUCTION,
// mutually exclusive (a concrete following segment has exactly one place-of-articulation value,
// so at most one tuple's right-environment ever matches a given position), `fsm_compose`-folding
// them SEQUENTIALLY is correct: tuple K's net only ever sees the placeholder if every earlier
// tuple in the fold left it untouched (not in ITS context), and once any ONE tuple rewrites it to
// a concrete segment, no LATER tuple's LHS (always the literal placeholder) can match it again.
// This is the exact same "feeding order" argument the OUTER stratum-level cascade
// (`compile_and_compose_rules`) already relies on — the fix is to apply it one level deeper too.
// =================================================================================================

/// Compile one [`RewriteRuleDef`] (all its subrules, all their alpha tuples) into ONE foma `Fsm`
/// (union of every subrule × tuple instance), and report the alpha-tuple expansion for every
/// alpha-bearing subrule (empty if the rule uses no alpha variables). Returns `None` if any
/// subrule's pattern needs an unsupported construct (module doc's scope list) — the CALLER
/// decides whether to skip that rule (reported uncovered) or treat the whole compile as failed;
/// this prototype's driver skips and reports.
///
/// Thin wrapper over [`compile_rewrite_rule_subset`] that includes every subrule (the pre-gating
/// behavior, unchanged for every existing caller). Builds a production [`ComposeBudget`] from
/// `HC_COMPOSE_*` env vars exactly once (mirrors `crate::emit::emit_with_precision`'s own
/// "read env in the production entry point only" convention) -- tests that need a deterministic,
/// tiny budget should call [`compile_rewrite_rule_subset`] directly with an explicit
/// [`ComposeBudget::with_caps`] instead.
pub fn compile_rewrite_rule(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    rule: &RewriteRuleDef,
) -> Result<Option<(Fsm, Vec<TupleReport>)>, ComposeError> {
    let budget = ComposeBudget::from_env();
    compile_rewrite_rule_subset(opts, g, alphabet, rule, &|_| true, &budget)
}

/// Identical to [`compile_rewrite_rule`], but SKIPS any subrule for which `allowed(subrule_index)`
/// is `false` (document order, `0`-based into `rule.subrules`) — the MPR/POS gating mechanism
/// (`crate::gate`): a subrule declaring `requiredPartsOfSpeech`/`requiredMPRFeatures`/
/// `excludedMPRFeatures` must not compile into a network branch that a NON-eligible lexical entry's
/// group can reach (module doc "static partition" design in `crate::gate`). Returns `None` if
/// EVERY subrule is either filtered out or hits an unsupported construct — the caller (per-group
/// rule cascade builder) treats that identically to "this rule doesn't fire in this group": the
/// whole rule is simply absent from the group's composed cascade (identity), not an error. This is
/// the same `None` the pre-gating code already used for "unsupported construct", so no NEW branch
/// is introduced at any call site — see [`compile_and_compose_rules_gated`]'s doc for the one
/// known imprecision this shares with the ungated path (a rule with one unsupported subrule and one
/// supported-but-gated subrule reports the WHOLE rule uncovered for every group, matching
/// [`compile_rewrite_rule`]'s own pre-existing all-or-nothing `?` short-circuit — not a regression).
///
/// `budget`: checked at two points (design doc `phase-b-compose-budget-design.md` §4) -- V3
/// immediately after [`resolve_alpha_tuples`] returns, BEFORE the (potentially expensive) per-tuple
/// compile loop runs (`AlphaTupleBudgetExceeded` if `report.surviving` exceeds
/// [`ComposeBudget::tuple_cap`]'s value, the cheapest-possible-predictor principle Fix 1's own
/// `EnumerationBudget` already uses); and V1, via [`compose_checked`], on every fold step of the
/// per-alpha-tuple union-by-composition below.
pub fn compile_rewrite_rule_subset(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    rule: &RewriteRuleDef,
    allowed: &dyn Fn(usize) -> bool,
    budget: &ComposeBudget,
) -> Result<Option<(Fsm, Vec<TupleReport>)>, ComposeError> {
    let mut net: Option<Fsm> = None;
    let mut reports: Vec<TupleReport> = Vec::new();

    for (subrule_index, subrule) in rule.subrules.iter().enumerate() {
        if !allowed(subrule_index) {
            continue;
        }
        // One shared occurrence counter per subrule: the LHS is textually shared across every
        // subrule of a rule but its alpha slots are numbered FRESH per subrule (HC's variable
        // scoping is per-subrule, module doc), so `lhs_slots` is (re)computed here, not hoisted
        // above the loop.
        let mut next_occurrence = 0usize;
        let Some(lhs_slots) = pattern_slots(g, &rule.lhs, &mut next_occurrence) else {
            return Ok(None);
        };
        let Some(rhs_slots) = pattern_slots(g, &subrule.rhs, &mut next_occurrence) else {
            return Ok(None);
        };
        let left_slots = match &subrule.left_env {
            Some(p) => match pattern_slots(g, p, &mut next_occurrence) {
                Some(s) => s,
                None => return Ok(None),
            },
            None => Vec::new(),
        };
        let right_slots = match &subrule.right_env {
            Some(p) => match pattern_slots(g, p, &mut next_occurrence) {
                Some(s) => s,
                None => return Ok(None),
            },
            None => Vec::new(),
        };

        let (assignments, report) = resolve_alpha_tuples(g, &[
            lhs_slots.as_slice(),
            rhs_slots.as_slice(),
            left_slots.as_slice(),
            right_slots.as_slice(),
        ]);
        // V3 (design doc §4): checked BEFORE the per-tuple compile loop -- the cheapest-possible
        // predictor, same principle as `EnumerationBudget`'s own "check the search result before
        // the expensive part".
        if report.surviving > budget.tuple_cap() {
            return Err(ComposeError::AlphaTupleBudgetExceeded {
                surviving: report.surviving,
                limit: budget.tuple_cap(),
                rule_xml_id: rule.xml_id.clone(),
            });
        }
        reports.push(report);

        for asg in &assignments {
            let lhs_text = render_slots(alphabet, &lhs_slots, asg);
            let rhs_text = render_slots(alphabet, &rhs_slots, asg);
            // Deletion (empty RHS pattern): foma spells "nothing" as `0` in a replace rule.
            let rhs_text = if rhs_text.is_empty() {
                "0".to_string()
            } else {
                rhs_text
            };
            let has_left = !left_slots.is_empty();
            let has_right = !right_slots.is_empty();
            let branch_regex = if !has_left && !has_right {
                format!("{lhs_text} -> {rhs_text}")
            } else {
                let left_text = render_slots(alphabet, &left_slots, asg);
                let right_text = render_slots(alphabet, &right_slots, asg);
                match (has_left, has_right) {
                    (true, true) => format!("{lhs_text} -> {rhs_text} || {left_text} _ {right_text}"),
                    (true, false) => format!("{lhs_text} -> {rhs_text} || {left_text} _"),
                    (false, true) => format!("{lhs_text} -> {rhs_text} || _ {right_text}"),
                    (false, false) => unreachable!(),
                }
            };
            let branch_net = fsm_parse_regex(opts, &branch_regex, None, None).unwrap_or_else(|| {
                panic!("foma rejected compiled regex for rule {}: {branch_regex:?}", rule.xml_id)
            });
            net = Some(match net {
                None => branch_net,
                // Sequential composition, NOT union — see the module-level doc above this
                // function for why union is wrong here.
                Some(prev) => compose_checked(
                    opts,
                    prev,
                    branch_net,
                    budget,
                    "compile_rewrite_rule_subset alpha-tuple fold",
                )?,
            });
        }
    }

    Ok(net.map(|n| (n, reports)))
}

/// Compile every `Rewrite`-kind [`PhonRuleDef`] in `stratum_prules` order into individual foma
/// nets and left-fold-compose them via [`fsm_compose`] (stratum/document order = feeding order —
/// prule4's assimilated output is prule5's own deletion-context input, verified against
/// `menulis`/`memukul` by hand in the prototype report). `Metathesis`-kind rules and any
/// `Rewrite` rule this module can't render are skipped, their `xml_id`s returned in `skipped` so
/// the caller can report them (never silently dropped).
///
/// Returns `None` if there are zero compilable rules at all (the composition would be a no-op —
/// callers should compose with an identity net instead of calling this).
///
/// Builds a production [`ComposeBudget`] from `HC_COMPOSE_*` env vars exactly once (mirrors
/// `crate::emit::emit_with_precision`'s own convention). Tests that need a deterministic, tiny
/// budget should call [`compile_and_compose_rules_with_budget`] directly instead.
///
/// Deliberately NOT given a final `minimize_checked` call (unlike `crate::gate::
/// compile_gated_grammar`, design doc §4 V2): `tests/p6_gate_parity.rs`'s
/// `amharic_gated_subrules_and_tuple_counts_unregressed` hard-asserts this function's return value
/// is BYTE IDENTICAL to the pre-Phase-B numbers (82 states / 1,110,358 arcs) with no minimize
/// applied by this function itself -- adding one here would change those counts (composing minimal
/// nets is not itself guaranteed minimal) and break that regression guard. This is a deliberate
/// deviation from the design doc's V2 text, which named this function alongside
/// `compile_gated_grammar` for the final-minimize-ownership change; see this crate's own task report
/// for the full reasoning. Callers that want a minimal composed rule net should call
/// `crate::compose_budget::minimize_checked` themselves (every example driver already does, via
/// `foma::minimize::fsm_minimize`, on the FULL `lexc .o. rules .o. cleanup` composition, not on this
/// function's return value alone).
pub fn compile_and_compose_rules(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    skipped: &mut Vec<String>,
    tuple_reports: &mut Vec<(String, Vec<TupleReport>)>,
) -> Result<Option<Fsm>, ComposeError> {
    let budget = ComposeBudget::from_env();
    compile_and_compose_rules_with_budget(
        opts,
        g,
        alphabet,
        prules_in_order,
        skipped,
        tuple_reports,
        &budget,
    )
}

/// [`compile_and_compose_rules`]'s core, with the [`ComposeBudget`] threaded in explicitly rather
/// than read from env -- what tests call directly (design doc §6: "explicit-caps constructors,
/// never env vars").
#[allow(clippy::too_many_arguments)]
pub fn compile_and_compose_rules_with_budget(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    skipped: &mut Vec<String>,
    tuple_reports: &mut Vec<(String, Vec<TupleReport>)>,
    budget: &ComposeBudget,
) -> Result<Option<Fsm>, ComposeError> {
    let mut composed: Option<Fsm> = None;
    for pr in prules_in_order {
        let PhonRuleDef::Rewrite(rule) = pr else {
            skipped.push(match pr {
                PhonRuleDef::Metathesis(m) => format!("{} (metathesis, unhandled)", m.xml_id),
                PhonRuleDef::Rewrite(_) => unreachable!(),
            });
            continue;
        };
        // Direction/mode fidelity note (module doc): every reference-grammar rule this prototype
        // has seen is `Iterative`/`LeftToRight`; both are compiled identically via plain foma
        // `->` (unioned per alpha-tuple, see [`compile_rewrite_rule`]'s doc). A `RightToLeft`
        // rule is silently mis-mapped rather than rejected here — a real gap, called out in the
        // prototype report, not hidden.
        let _ = (rule.mode, rule.dir); // read for the record; not branched on (see doc above)
        match compile_rewrite_rule_subset(opts, g, alphabet, rule, &|_| true, budget)? {
            Some((net, reports)) => {
                tuple_reports.push((rule.xml_id.clone(), reports));
                composed = Some(match composed {
                    None => net,
                    Some(prev) => {
                        compose_checked(opts, prev, net, budget, "compile_and_compose_rules cascade fold")?
                    }
                });
            }
            None => skipped.push(rule.xml_id.clone()),
        }
    }
    Ok(composed)
}

/// Identical to [`compile_and_compose_rules`], but for ONE GATING GROUP (`crate::gate`): for every
/// `Rewrite`-kind rule at position `rule_pos` in `prules_in_order`, `subrule_ok(rule_pos, sub_idx)`
/// decides whether that specific subrule is included for THIS group (module doc: a group is a set
/// of lexical entries that agree on every gated subrule's applicability, so ungated subrules always
/// pass `subrule_ok` unconditionally — only `crate::gate`'s own gated-subrule list ever returns
/// `false`). A rule whose every subrule is filtered out for this group is skipped exactly like an
/// unsupported-construct rule (absent from the group's cascade, i.e. identity for this group) —
/// see [`compile_rewrite_rule_subset`]'s doc.
///
/// Builds a production [`ComposeBudget`] from `HC_COMPOSE_*` env vars exactly once -- same
/// convention as [`compile_and_compose_rules`]. Tests should call
/// [`compile_and_compose_rules_gated_with_budget`] directly instead.
#[allow(clippy::too_many_arguments)]
pub fn compile_and_compose_rules_gated(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    subrule_ok: &dyn Fn(usize, usize) -> bool,
    skipped: &mut Vec<String>,
    tuple_reports: &mut Vec<(String, Vec<TupleReport>)>,
) -> Result<Option<Fsm>, ComposeError> {
    let budget = ComposeBudget::from_env();
    compile_and_compose_rules_gated_with_budget(
        opts,
        g,
        alphabet,
        prules_in_order,
        subrule_ok,
        skipped,
        tuple_reports,
        &budget,
    )
}

/// [`compile_and_compose_rules_gated`]'s core, with the [`ComposeBudget`] threaded in explicitly
/// rather than read from env -- what `crate::gate::compile_gated_grammar_with_budget` and tests
/// call directly, so a whole gated-grammar compile shares ONE budget across every group's cascade.
#[allow(clippy::too_many_arguments)]
pub fn compile_and_compose_rules_gated_with_budget(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    subrule_ok: &dyn Fn(usize, usize) -> bool,
    skipped: &mut Vec<String>,
    tuple_reports: &mut Vec<(String, Vec<TupleReport>)>,
    budget: &ComposeBudget,
) -> Result<Option<Fsm>, ComposeError> {
    let mut composed: Option<Fsm> = None;
    for (rule_pos, pr) in prules_in_order.iter().enumerate() {
        let PhonRuleDef::Rewrite(rule) = pr else {
            skipped.push(match pr {
                PhonRuleDef::Metathesis(m) => format!("{} (metathesis, unhandled)", m.xml_id),
                PhonRuleDef::Rewrite(_) => unreachable!(),
            });
            continue;
        };
        let _ = (rule.mode, rule.dir); // see compile_and_compose_rules's own doc on this
        let allowed = |sub_idx: usize| subrule_ok(rule_pos, sub_idx);
        match compile_rewrite_rule_subset(opts, g, alphabet, rule, &allowed, budget)? {
            Some((net, reports)) => {
                tuple_reports.push((rule.xml_id.clone(), reports));
                composed = Some(match composed {
                    None => net,
                    Some(prev) => compose_checked(
                        opts,
                        prev,
                        net,
                        budget,
                        "compile_and_compose_rules_gated cascade fold",
                    )?,
                });
            }
            None => skipped.push(rule.xml_id.clone()),
        }
    }
    Ok(composed)
}

/// `true` iff `rule.mode`/`rule.dir` are the only combination this prototype claims fidelity for.
pub fn is_fully_supported_shape(rule: &RewriteRuleDef) -> bool {
    matches!(rule.mode, RewriteMode::Iterative) && matches!(rule.dir, Dir::LeftToRight)
}

/// Convenience re-export so the driver doesn't need a second `use` line for the one subrule field
/// this module reads directly (`mode`/`dir` are read via [`is_fully_supported_shape`] instead).
pub type Subrule = RewriteSubruleDef;

#[cfg(test)]
mod compose_budget_tests {
    //! `docs/fst-plan/phase-b-compose-budget-design.md` §6's own test plan for this module: a
    //! hand-authored, minimal grammar with ONE alpha-bound rewrite rule whose RHS occurrence draws
    //! from a natural class with a KNOWN, exact member count (6 -- an "Any"-style
    //! `FeatureNaturalClass` with zero explicit `FeatureValue` constraints of its own, so
    //! `class_members` returns every segment in the table; see [`compile_rewrite_rule_subset`]'s
    //! alpha-resolution doc). This gives [`resolve_alpha_tuples`] a `raw_product`/`surviving` of
    //! EXACTLY 6 by construction (a single occurrence trivially agrees with itself, module doc on
    //! [`AlphaAssignment`]), which every test below relies on.
    use super::*;
    use crate::compose_budget::NetSizeMeasure;

    /// 6 segments (`c1`..`c6`), all matching the sole natural class `ncBig` (zero explicit
    /// `FeatureValue`s -- an "Any" class, same shape as the real Indonesian grammar's own `nc1`),
    /// each also carrying a nonzero `featA` bit (required for `resolve_alpha_tuples`'s own
    /// self-agreement check to pass: a segment whose alpha-bound feature lane is entirely UNSET
    /// would fail `lane_value(cd, feat) & lane_value(cd, feat) != 0` against itself). One
    /// `PhonologicalRule` (`prule_alpha`): LHS = fixed `c1`, RHS = `ncBig` alpha-bound to `var1`, no
    /// environment -- the minimal shape that produces >1 alpha assignment without needing a second
    /// occurrence.
    const SYNTH_ALPHA_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ComposeBudgetAlphaFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featA">
        <Name>dummy</Name>
        <Symbols>
          <Symbol id="symA1">a</Symbol>
          <Symbol id="symA2">b</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>a</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
        <SegmentDefinition id="c3"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
        <SegmentDefinition id="c4"><Representations><Representation>d</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
        <SegmentDefinition id="c5"><Representations><Representation>e</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
        <SegmentDefinition id="c6"><Representations><Representation>f</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncBig">
        <Name>Any</Name>
      </FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule_alpha">
        <Name>synthetic alpha rule</Name>
        <VariableFeatures>
          <VariableFeature id="var1" name="a" phonologicalFeature="featA" />
        </VariableFeatures>
        <PhoneticInput>
          <PhoneticSequence>
            <Segment segment="c1" />
          </PhoneticSequence>
        </PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput>
              <PhoneticSequence>
                <SimpleContext naturalClass="ncBig">
                  <AlphaVariables>
                    <AlphaVariable variableFeature="var1" />
                  </AlphaVariables>
                </SimpleContext>
              </PhoneticSequence>
            </PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prule_alpha">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entry1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="allo1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>dummy</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    fn synth_alpha_grammar() -> Grammar {
        pg_grammar::load(SYNTH_ALPHA_XML)
            .unwrap_or_else(|e| panic!("failed to load synthetic alpha fixture: {e}\n{SYNTH_ALPHA_XML}"))
    }

    fn synth_alpha_rule(g: &Grammar) -> &RewriteRuleDef {
        for pr in &g.prules {
            if let PhonRuleDef::Rewrite(r) = pr {
                if r.xml_id == "prule_alpha" {
                    return r;
                }
            }
        }
        panic!("prule_alpha not found in synthetic fixture");
    }

    /// V3 (design doc §4): `resolve_alpha_tuples` for `prule_alpha` surfaces EXACTLY 6 surviving
    /// assignments (module doc); a `tuple_cap` below that must trip `AlphaTupleBudgetExceeded`
    /// BEFORE any per-tuple compile work happens.
    #[test]
    fn alpha_tuple_budget_trips_on_synthetic_rule() {
        let g = synth_alpha_grammar();
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();
        let rule = synth_alpha_rule(&g);

        let budget = ComposeBudget::with_caps(usize::MAX, usize::MAX, 3, usize::MAX, usize::MAX, None);
        let err = compile_rewrite_rule_subset(&opts, &g, &alphabet, rule, &|_| true, &budget)
            .expect_err("6 surviving tuples must exceed a tuple_cap of 3");
        match err {
            ComposeError::AlphaTupleBudgetExceeded { surviving, limit, rule_xml_id } => {
                assert_eq!(surviving, 6, "synthetic fixture's ncBig class must have exactly 6 members");
                assert_eq!(limit, 3);
                assert_eq!(rule_xml_id, "prule_alpha");
            }
            other => panic!("expected AlphaTupleBudgetExceeded, got {other:?}"),
        }
    }

    /// V1 (design doc §4): composing the 6 per-tuple branch nets left-to-right must trip
    /// `NetSizeExceeded` on the second fold (`compose_checked`'s own site inside
    /// `compile_rewrite_rule_subset`) once `state_cap` is small enough -- the tuple budget itself
    /// stays unbounded here so this test isolates the state-size check specifically.
    #[test]
    fn state_budget_trips_on_tiny_cascade() {
        let g = synth_alpha_grammar();
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();
        let rule = synth_alpha_rule(&g);

        // NOTE: these single-occurrence branch nets ("c1 -> cK" for varying K) each compile/compose
        // to a tiny (often single-state, self-looping) automaton -- composing them sequentially
        // does not grow the state count the way a real multi-rule cascade would, so this test uses
        // `state_cap=0` (guaranteed to trip on ANY non-empty composed net) rather than the design
        // doc's illustrative "cap=2", which this specific hand-authored fixture's nets are too
        // small to cross.
        let budget = ComposeBudget::with_caps(0, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None);
        let err = compile_rewrite_rule_subset(&opts, &g, &alphabet, rule, &|_| true, &budget)
            .expect_err("composing 6 branch nets must exceed a state_cap of 0");
        assert!(
            matches!(err, ComposeError::NetSizeExceeded { measure: NetSizeMeasure::States, .. }),
            "expected NetSizeExceeded(States), got {err:?}"
        );
    }

    /// [`ComposeBudget::unbounded`] must never trip on this small fixture -- proves the checked
    /// wrappers are pure passthrough when every cap is `usize::MAX` and `step_timeout` is `None`.
    #[test]
    fn unbounded_budget_never_trips_on_small_fixture() {
        let g = synth_alpha_grammar();
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();
        let rule = synth_alpha_rule(&g);

        let budget = ComposeBudget::unbounded();
        let (net, reports) = compile_rewrite_rule_subset(&opts, &g, &alphabet, rule, &|_| true, &budget)
            .expect("unbounded budget must never trip")
            .expect("synthetic rule must compile");
        assert!(net.statecount > 0);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].surviving, 6);
    }
}
