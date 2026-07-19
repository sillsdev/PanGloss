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

use foma::constructions::fsm_compose;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;

use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::{
    Dir, Grammar, NaturalClassKind, Pattern, PatternNode, PhonRuleDef, RewriteMode,
    RewriteRuleDef, RewriteSubruleDef, VarId,
};

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
/// behavior, unchanged for every existing caller).
pub fn compile_rewrite_rule(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    rule: &RewriteRuleDef,
) -> Option<(Fsm, Vec<TupleReport>)> {
    compile_rewrite_rule_subset(opts, g, alphabet, rule, &|_| true)
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
pub fn compile_rewrite_rule_subset(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    rule: &RewriteRuleDef,
    allowed: &dyn Fn(usize) -> bool,
) -> Option<(Fsm, Vec<TupleReport>)> {
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
        let lhs_slots = pattern_slots(g, &rule.lhs, &mut next_occurrence)?;
        let rhs_slots = pattern_slots(g, &subrule.rhs, &mut next_occurrence)?;
        let left_slots = match &subrule.left_env {
            Some(p) => pattern_slots(g, p, &mut next_occurrence)?,
            None => Vec::new(),
        };
        let right_slots = match &subrule.right_env {
            Some(p) => pattern_slots(g, p, &mut next_occurrence)?,
            None => Vec::new(),
        };

        let (assignments, report) = resolve_alpha_tuples(g, &[
            lhs_slots.as_slice(),
            rhs_slots.as_slice(),
            left_slots.as_slice(),
            right_slots.as_slice(),
        ]);
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
                Some(prev) => fsm_compose(opts, prev, branch_net),
            });
        }
    }

    net.map(|n| (n, reports))
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
pub fn compile_and_compose_rules(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    skipped: &mut Vec<String>,
    tuple_reports: &mut Vec<(String, Vec<TupleReport>)>,
) -> Option<Fsm> {
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
        match compile_rewrite_rule(opts, g, alphabet, rule) {
            Some((net, reports)) => {
                tuple_reports.push((rule.xml_id.clone(), reports));
                composed = Some(match composed {
                    None => net,
                    Some(prev) => fsm_compose(opts, prev, net),
                });
            }
            None => skipped.push(rule.xml_id.clone()),
        }
    }
    composed
}

/// Identical to [`compile_and_compose_rules`], but for ONE GATING GROUP (`crate::gate`): for every
/// `Rewrite`-kind rule at position `rule_pos` in `prules_in_order`, `subrule_ok(rule_pos, sub_idx)`
/// decides whether that specific subrule is included for THIS group (module doc: a group is a set
/// of lexical entries that agree on every gated subrule's applicability, so ungated subrules always
/// pass `subrule_ok` unconditionally — only `crate::gate`'s own gated-subrule list ever returns
/// `false`). A rule whose every subrule is filtered out for this group is skipped exactly like an
/// unsupported-construct rule (absent from the group's cascade, i.e. identity for this group) —
/// see [`compile_rewrite_rule_subset`]'s doc.
#[allow(clippy::too_many_arguments)]
pub fn compile_and_compose_rules_gated(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    subrule_ok: &dyn Fn(usize, usize) -> bool,
    skipped: &mut Vec<String>,
    tuple_reports: &mut Vec<(String, Vec<TupleReport>)>,
) -> Option<Fsm> {
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
        match compile_rewrite_rule_subset(opts, g, alphabet, rule, &allowed) {
            Some((net, reports)) => {
                tuple_reports.push((rule.xml_id.clone(), reports));
                composed = Some(match composed {
                    None => net,
                    Some(prev) => fsm_compose(opts, prev, net),
                });
            }
            None => skipped.push(rule.xml_id.clone()),
        }
    }
    composed
}

/// `true` iff `rule.mode`/`rule.dir` are the only combination this prototype claims fidelity for.
pub fn is_fully_supported_shape(rule: &RewriteRuleDef) -> bool {
    matches!(rule.mode, RewriteMode::Iterative) && matches!(rule.dir, Dir::LeftToRight)
}

/// Convenience re-export so the driver doesn't need a second `use` line for the one subrule field
/// this module reads directly (`mode`/`dir` are read via [`is_fully_supported_shape`] instead).
pub type Subrule = RewriteSubruleDef;
