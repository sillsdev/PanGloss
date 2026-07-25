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
//! - `RewriteMode::Simultaneous` whose subrules the `simultaneous.subrule-overlap` predicate (D3,
//!   `crate::capability`) cannot prove pairwise non-overlapping (self-opaquing, an unresolved
//!   overlap, or an unsupported pattern node in a lowered span) — see "`RewriteMode::Simultaneous`:
//!   compiling the ADMITTED case" below for the (now real) admitted case.
//! - MPR gating (`required_mpr`/`excluded_mpr` on a subrule) — flag-diacritic emission is P6
//!   mainline work per the plan (`§P6` item 1's own text), not attempted in this slice.
//!
//! ## `Dir::RightToLeft`: the reversal construction (`openspec/changes/
//! compile-right-to-left-rewrites`)
//! `Dir::RightToLeft` used to be honestly skipped (the same `Ok(None)` treatment `Simultaneous`
//! still gets); this change gives it real, direction-faithful semantics via the STANDARD
//! finite-state technique for "prefer the rightmost, not leftmost, non-overlapping match" (Beesley
//! & Karttunen, *Finite State Morphology*, ch. 6 "Directional replacement rules"; ADR 0001, `docs/
//! adr/0001-honest-capability-boundary.md`, "confirm-only-by-default"): reverse ∘ compile(mirror
//! rule) ∘ reverse, NOT "compile as if `LeftToRight`".
//!
//! **The mirror rule.** Foma's native `->` only ever prefers the LEFTMOST of several
//! non-overlapping candidate matches (there is no built-in "prefer rightmost" operator). To get
//! rightmost preference, [`compile_rtl_branch_net`] builds the MIRROR IMAGE of the rule — reverse
//! the LHS's own slot order, reverse the RHS's own slot order, and SWAP the two environments while
//! ALSO reversing each one's own slot order (`left_env' = reverse(right_env)`, `right_env' =
//! reverse(left_env)`) — compiles that mirror rule with the SAME plain-`->` machinery
//! [`render_branch_regex`] already uses for `LeftToRight`, and then calls [`fsm_reverse`] on the
//! resulting `Fsm`. `fsm_reverse`'s own contract (`foma::reverse`'s doc: "all original state
//! numbers are shifted up by 1... label sides are NOT swapped") means: for a transducer whose own
//! upper/lower tapes spell `reverse(S)`/`reverse(S')` when read forward, `fsm_reverse` of it spells
//! `S`/`S'` when read forward — i.e. reversing a network that operates on REVERSED strings gives
//! back a network that operates on NORMAL strings, but the internal left-to-right preference that
//! was baked into the mirror compile (over the reversed alphabet) becomes a right-to-left
//! preference over the real, un-reversed string. Environments keep their ordinary, un-reversed
//! meaning in the FINAL network (`left_env` is still "precedes the target in the real string") —
//! the swap+reverse only happens in the INTERMEDIATE mirror-rule text; see
//! [`compile_rtl_branch_net`]'s own doc for the worked "aa -> b" example this construction is
//! checked against.
//!
//! **The safety-net union (a documented, conservative judgment call).** `pg_rules::rewrite`'s own
//! `Iterative` synthesis/analysis loops (`syn_feature`/`syn_narrow`/`ana_feature`/…) pick which
//! candidate span to act on first via `all_spans`'/`candidates.sort_unstable()`'s own ASCENDING
//! sort — i.e. this repo's current full-HC oracle is, empirically, direction-BLIND for the "which
//! overlapping match wins" question (verified directly: a hand-built `aa -> b` rule applied to
//! `"aaa"` synthesizes to `"ba"` whether the rule is declared `LeftToRight` or
//! `rightToLeftIterative`). ADR 0001: *"Where the oracle itself is unverified for a configuration...
//! the configuration is unsupported by definition."* Rather than let a THEORETICALLY-faithful
//! reversal-only compile under-propose relative to what this repo's own confirm engine actually
//! requires for recall (the reversal-only net for `aa -> b`/`RightToLeft` maps `"aaa"` to `"ab"`,
//! never `"ba"` — so it would never even PROPOSE the lexical form the current oracle confirms for
//! surface `"ba"`), [`compile_rtl_branch_net`] returns `fsm_union(plain_LTR_style_net,
//! reversed_net)`: the SAME plain construction [`render_branch_regex`] already gives `LeftToRight`
//! (a proven-safe floor, since the oracle treats every direction identically today) UNIONED with
//! the genuinely-reversed net (so the construction really is direction-aware, differs from a plain
//! `LeftToRight` compile on any input where the two branches disagree, and is READY the day
//! `pg_rules::rewrite`'s own pick-order gets a direction-aware fix — a follow-on outside this
//! single-owner file's scope, flagged, not fixed here). Both branches are already COMPLETE,
//! obligatory replace transducers (each has no "did nothing" identity path at a position its own
//! context matches), so `fsm_union`ing them adds no spurious third "nothing happened" path — see
//! [`compile_rtl_branch_net`]'s own doc for why this differs from the alpha-tuple union-is-wrong
//! finding above.
//!
//! ## `RewriteMode::Simultaneous`: compiling the ADMITTED case (`openspec/changes/
//! compile-simultaneous-rewrites`)
//! `RewriteMode::Simultaneous` used to be honestly skipped UNCONDITIONALLY (`Ok(None)` for every
//! such rule, regardless of subrule shape — the same treatment metathesis and an unsupported
//! pattern construct get). It still stays that way for a rule whose subrules the
//! `simultaneous.subrule-overlap` predicate (D3, `crate::capability::
//! SimultaneousSubruleOverlapPredicate`, already built by Stage 1B) cannot prove pairwise
//! non-overlapping. What changes here: for a rule the predicate DOES admit —
//! [`is_fully_supported_shape`] now asks `crate::capability::
//! simultaneous_rule_admitted_for_compile` (that function's own doc: the SAME D3 proof, freshly
//! computed, sharing its algorithm with the capability gate's own predicate so the two can never
//! disagree) — this file's EXISTING plain/iterative sequential-compose machinery is reused UNCHANGED,
//! not reimplemented: no new branch net construction, no new fold shape, nothing analogous to
//! [`compile_rtl_branch_net`]'s mirror-plus-reverse-plus-union.
//!
//! **Why reuse, not a new algorithm, is actually correct here (not merely convenient).** D3's own
//! Admit boundary is defined EXACTLY as "no two subrules' environments can ever match at the same
//! input position" — precisely the condition under which HC's true `Simultaneous` semantics (find
//! every match against ONE untouched input snapshot, then apply them all — `rust/docs/
//! p13-simultaneous-design.md` §1.1's `SimultaneousPhonologicalPatternRule.Apply`) and a sequential
//! per-subrule fold (this file's existing `Iterative`-labeled machinery) produce IDENTICAL output:
//! with no shared focus position in contention, subrule application order can never change which
//! subrule wins where, so "compose subrule 1's net, then subrule 2's net" (what this file already
//! does for `Iterative`) and "collect all subrules' matches against the original input, then apply
//! all of them" (true `Simultaneous`) coincide. A second, independently-confirmed reason this
//! reuse is faithful, not just permitted: a plain foma `->` replace rule is ITSELF a single-pass,
//! snapshot-style construction (Beesley & Karttunen's classical replace-rule automaton finds every
//! non-overlapping match against the rule's own input tape and rewrites them all in one
//! transduction — it cannot self-feed within one compiled expression the way `pg-rules`'
//! `syn_feature`'s re-scan-after-every-mutation loop can, `p13-simultaneous-design.md` §2.2/§2.3's
//! own finding that `syn_epenthesis` is "already Simultaneous-shaped" for exactly this reason). So
//! this file's foma-`->`-based compile was ALREADY structurally closer to true `Simultaneous`
//! semantics than to HC's `Iterative` re-scan semantics, for ANY rule it has ever compiled — the
//! `Iterative` label on the existing machinery names which HC mode it happens to have been used
//! for so far, not an inherent re-scan behavior the compiled net exhibits.
//!
//! **What `pg_rules::rewrite` (the confirm engine) actually does for `Simultaneous`.** Unlike the
//! `RightToLeft` case above, `pg_rules::rewrite` is NOT mode-blind here: it dispatches
//! `Kind::Feature`/`Kind::Narrow` synthesis to genuinely distinct `sim_feature`/`sim_narrow`
//! functions (vs. `syn_feature`/`syn_narrow` for `Iterative`), and its analysis side wraps
//! `ana_feature`/`ana_epenthesis` in a repeat-until-fixpoint loop whenever a subrule is
//! `self_opaquing` (`p13-simultaneous-design.md` §1.3/§4.3-4.4) — a real, load-bearing mode
//! dependence, ported and shipped (P13), not a gap this change needs to patch around. D3's own
//! `self_opaquing`-Refuse early-out is exactly what keeps the ADMITTED case inside the region where
//! this asymmetry never actually bites: `self_opaquing` is REQUIRED true for the repeat-wrapper to
//! ever trigger, and D3 refuses any pair containing one (this crate's own
//! `simultaneous_rule_admitted_for_compile` additionally refuses a LONE self-opaquing subrule too,
//! stricter than D3's own pairwise-only algorithm — see that function's doc for why). So for every
//! rule this file now actually compiles under `Simultaneous`, confirm's analysis side runs
//! `ana_feature`/`ana_epenthesis` exactly once, per subrule, with no fixpoint loop — the SAME shape
//! `Iterative` mode's analysis already uses (`p13-simultaneous-design.md` §1.3: "`ApplicationMode`
//! has zero effect on which pattern rule analysis uses" for Feature subrules). No safety-net union
//! is needed here (contrast [`compile_rtl_branch_net`]'s own documented judgment call): there is no
//! known faithfulness gap between what this file compiles and what confirm accepts for the admitted
//! case, so no superset-widening is required to stay recall-safe.

use std::collections::HashSet;

use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::reverse::fsm_reverse;
use foma::types::Fsm;

use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::{
    Dir, Grammar, NaturalClassKind, Pattern, PatternNode, PhonRuleDef, PRuleId, RewriteMode,
    RewriteRuleDef, RewriteSubruleDef, VarId,
};

use crate::compose_budget::{compose_checked, union_checked, ComposeBudget, ComposeError};

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
///
/// `pub(crate)`: reused by [`crate::lower`] (Stage 1B, `lower-fst-pattern-environments`) rather
/// than re-derived -- see that module's own doc for exactly which pieces of this file it borrows.
///
/// `Clone` (`openspec/changes/compile-right-to-left-rewrites`): the RTL reversal construction
/// needs a REVERSED copy of a subrule's own slot lists (`reversed_slots`, below) alongside the
/// original document-order lists it builds the safety-net `LeftToRight`-style branch from -- see
/// [`compile_rtl_branch_net`]'s doc.
#[derive(Clone)]
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
}

/// Walk `pattern`'s nodes into [`Slot`]s, numbering each `Alpha` occurrence sequentially from
/// `*next_occurrence` (shared across LHS/RHS/left-env/right-env for one subrule — see
/// [`compile_rewrite_rule`]). Returns `None` (uncovered) on `Quantifier`/`Segments`/`Anchor`/
/// disagree-polarity `Context` — this prototype's documented scope line (module doc).
///
/// `table`: every `Context` node's `NatClassId` is resolved against THIS table
/// ([`class_members`]), never an implicit grammar-wide default
/// (`openspec/changes/fix-multitable-fst-compilation`, design.md: "table zero is never an
/// implicit default"). The caller is responsible for choosing the RIGHT table — see
/// [`owning_table`]'s own doc for how [`compile_rewrite_rule_subset`] picks it (the rule's own
/// stratum's `StratumDef::table`), and [`crate::lower::lower_span`]'s call sites for how that
/// module picks it (`alphabet.table()`, already the correct per-caller table by that module's own
/// contract). This replaces the prior `table_of(g, _sc)` helper, which unconditionally returned
/// `&g.char_tables[0]` regardless of which table the pattern's own rule actually belonged to — the
/// exact bug `tests/phase_c_multi_table.rs` (formerly a DETECT-WRONG gate, now inverted to assert
/// the correct compile) pins.
///
/// `pub(crate)`: reused by [`crate::lower`] (Stage 1B) to lower a subrule's environment/focus
/// pattern the SAME way this file already lowers LHS/RHS/environment patterns — one pattern
/// semantics, not two independently-maintained ones.
pub(crate) fn pattern_slots(
    g: &Grammar,
    table: &CharDefTable,
    pattern: &Pattern,
    next_occurrence: &mut usize,
) -> Option<Vec<Slot>> {
    let mut out = Vec::with_capacity(pattern.nodes.len());
    for node in &pattern.nodes {
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
            PatternNode::Quantifier { .. }
            | PatternNode::Segments { .. }
            | PatternNode::Anchor(_) => {
                return None;
            }
        }
    }
    Some(out)
}

/// Resolves `rule`'s OWNING [`CharDefTable`] via its owning stratum's `StratumDef::table`
/// (`openspec/changes/fix-multitable-fst-compilation`, design.md: "Every compiled rule carries its
/// owning character-table identity explicitly; table zero is never an implicit default").
///
/// `rule` is looked up in `g.prules` by `xml_id` (document-unique, per the DTD's own `xs:ID`
/// discipline for every element's `id=` attribute — `pg_grammar::load`'s own convention) rather
/// than by pointer identity: [`compile_rewrite_rule_subset`] receives `rule: &RewriteRuleDef`
/// already unwrapped from its caller's own `&PhonRuleDef` reference, and every existing caller
/// building a `prules_in_order` list (`gate.rs`, every `examples/p6_*` driver, every
/// `tests/phase_c_*` gate) derives it by walking `g.strata`'s own `prules: Vec<PRuleId>` fields in
/// stratum order — so the rule THOSE callers ask about always originates from EXACTLY one
/// stratum's own `prules` list, by construction of how they build that list.
///
/// Returns `None` (never panics, never falls back to an implicit table-zero guess) when `rule`
/// cannot be found in `g.prules` at all, OR — a real, DTD-legal shape this crate's own minimal
/// unit fixtures exercise (a `<PhonologicalRule>` declared but not referenced by ANY `<Stratum
/// phonologicalRules="...">`) — when no stratum's own `prules` list contains it: a rule
/// unreachable from any stratum's own cascade has no owning table to report, and the conservative
/// choice (matching this module's whole "approximate only upward, report don't hide" discipline)
/// is an honest `None` a caller can route to its OWN "uncovered"/`Unsupported` handling, never a
/// silent guess. [`compile_rewrite_rule_subset`] treats `None` exactly like an unsupported pattern
/// construct (`Ok(None)`, reported `skipped` by its own caller); `capability.rs`'s
/// `lower_subrule_span` rounds it to [`LoweredSpan::Unsupported`] (D3's own "any approximation
/// rounds toward Refuse").
pub(crate) fn owning_table<'g>(g: &'g Grammar, rule: &RewriteRuleDef) -> Option<&'g CharDefTable> {
    let idx = g
        .prules
        .iter()
        .position(|pr| matches!(pr, PhonRuleDef::Rewrite(r) if r.xml_id == rule.xml_id))?;
    let target = PRuleId(idx as u32);
    let stratum = g.strata.iter().find(|s| s.prules.contains(&target))?;
    Some(&g.char_tables[stratum.table.0 as usize])
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
/// `pub(crate)`: reused by [`crate::lower`] (Stage 1B) for the SAME reason [`pattern_slots`] is —
/// a subrule's span lowering needs the identical joint-agreement resolution this file already
/// gives LHS/RHS/environment compilation, not a second implementation of the same semantics.
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
///
/// `pub(crate)`: reused by [`crate::lower`] (Stage 1B) — same rendering, same PUA-token load-
/// bearing space-separation rule, for the environment/focus text it feeds `fsm_parse_regex`.
pub(crate) fn render_slots(
    alphabet: &SegAlphabet,
    slots: &[Slot],
    assignment: &AlphaAssignment,
) -> String {
    let mut pieces: Vec<String> = Vec::with_capacity(slots.len());
    for slot in slots {
        let piece = match slot {
            Slot::Fixed(cd) => alphabet.token(*cd).to_string(),
            Slot::Union(members) => {
                if members.len() == 1 {
                    alphabet.token(members[0]).to_string()
                } else {
                    let inner: Vec<String> = members
                        .iter()
                        .map(|m| alphabet.token(*m).to_string())
                        .collect();
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

/// `slots` in REVERSE document order (`openspec/changes/compile-right-to-left-rewrites`'s mirror-
/// rule construction, module doc): a plain `.iter().rev().cloned()` copy, never mutated in place,
/// so the caller's own document-order `Vec<Slot>` (needed unchanged for the safety-net plain
/// branch, [`compile_rtl_branch_net`]'s doc) is untouched.
fn reversed_slots(slots: &[Slot]) -> Vec<Slot> {
    slots.iter().rev().cloned().collect()
}

/// Renders one COMPLETE branch's xre source text (`"LHS -> RHS"`, optionally with `|| L _`/`|| _
/// R`/`|| L _ R`), given slot lists ALREADY in the final order/environment-role this branch wants
/// to render (document order for the ordinary `LeftToRight`-style branch; mirrored — reversed and
/// left/right-swapped — for [`compile_rtl_branch_net`]'s intermediate mirror-rule compile).
///
/// Empty `lhs`/`rhs` render as foma's own epenthesis/deletion literals, `"[..]"`/`"0"` (foma's xre
/// grammar rejects a literally-blank LHS/RHS operand — confirmed empirically, `docs/fst-plan/
/// p6-prototype-report.md`-style bisection: `"0 -> x || a _ b"` silently compiles to a rule that
/// NEVER inserts on either tape, while `"[..] -> x || a _ b"`, foma's own documented epenthesis
/// notation — `foma::rewrite`'s own test `rewrite_epenthesis`, `"[..] -> x"` inserting `x` at
/// every position — behaves correctly). The RHS `"0"` deletion literal was already this file's own
/// convention before this change (deletion IS exercised by the reference grammars); the LHS
/// `"[..]"` epenthesis literal is new here — no existing caller ever renders an empty-LHS branch
/// (`rule.lhs.nodes.is_empty()` is [`CharacteristicKind::Epenthesis`](crate::capability::
/// CharacteristicKind::Epenthesis)'s own trigger, still `FailClosed`-placeholder'd in
/// `capability.rs` for unrelated reasons — this fix only makes THIS FILE's own compile mechanics
/// epenthesis-capable; it does not by itself flip that placeholder), so this changes no existing
/// test's compiled output.
fn render_branch_regex(
    alphabet: &SegAlphabet,
    lhs_slots: &[Slot],
    rhs_slots: &[Slot],
    left_slots: &[Slot],
    right_slots: &[Slot],
    asg: &AlphaAssignment,
) -> String {
    let lhs_text = render_slots(alphabet, lhs_slots, asg);
    let lhs_text = if lhs_text.is_empty() {
        "[..]".to_string()
    } else {
        lhs_text
    };
    let rhs_text = render_slots(alphabet, rhs_slots, asg);
    let rhs_text = if rhs_text.is_empty() {
        "0".to_string()
    } else {
        rhs_text
    };
    let has_left = !left_slots.is_empty();
    let has_right = !right_slots.is_empty();
    if !has_left && !has_right {
        format!("{lhs_text} -> {rhs_text}")
    } else {
        let left_text = render_slots(alphabet, left_slots, asg);
        let right_text = render_slots(alphabet, right_slots, asg);
        match (has_left, has_right) {
            (true, true) => format!("{lhs_text} -> {rhs_text} || {left_text} _ {right_text}"),
            (true, false) => format!("{lhs_text} -> {rhs_text} || {left_text} _"),
            (false, true) => format!("{lhs_text} -> {rhs_text} || _ {right_text}"),
            (false, false) => unreachable!("has_left || has_right guarded this branch"),
        }
    }
}

/// Compiles one branch (one subrule, one alpha-tuple assignment) into a foma `Fsm`, dispatching on
/// `dir` (module doc, "`Dir::RightToLeft`: the reversal construction"):
/// - `Dir::LeftToRight`: [`render_branch_regex`] over the slots AS GIVEN (document order), compiled
///   with plain foma `->` — byte-identical to what this file has always done (no behavior change
///   for any `LeftToRight` rule).
/// - `Dir::RightToLeft`: `fsm_union(plain_net, reversed_net)` where `plain_net` is the SAME
///   `LeftToRight`-style compile (the safety-net floor, module doc) and `reversed_net` is
///   [`fsm_reverse`] of the MIRROR rule's compile — LHS/RHS reversed, environments swapped-and-
///   reversed (`left_env' = reverse(right_env)`, `right_env' = reverse(left_env)`).
///
/// # Worked example (`tests/phase_c_right_to_left.rs`'s own `rtl-distinct-leftmost-rightmost`
/// witness, "aa -> b" on "aaa")
/// Plain `LeftToRight` compile of `"aa -> b"` prefers the LEFTMOST non-overlapping match: applied
/// to `"aaa"` it yields `"ba"` (positions 0-1 replaced, trailing "a" survives). The mirror rule
/// reverses LHS ("aa" reversed is still "aa", a palindrome) and RHS ("b" reversed is "b") — same
/// xre text — so `fsm_reverse` of that mirror compile yields a network that, on the SAME un-
/// reversed input `"aaa"`, prefers the RIGHTMOST non-overlapping match instead: `"ab"` (positions
/// 1-2 replaced, leading "a" survives). `reversed_net` alone therefore genuinely differs from
/// `plain_net` on this input (proof the construction is not "compiled as LeftToRight"); the
/// returned `fsm_union(plain_net, reversed_net)` accepts BOTH `"ba"` and `"ab"` as valid rewrites
/// of `"aaa"` (module doc's safety-net rationale).
///
/// No spurious THIRD "did nothing" path is introduced by this union (contrast the alpha-tuple
/// union-is-wrong finding two sections up): both `plain_net` and `reversed_net` are already
/// COMPLETE, OBLIGATORY replace transducers over the FULL rule (not a partition of a shared
/// context space the way per-tuple branches are) — neither one has an identity-elsewhere escape
/// hatch at a position where its own LHS/environment matches, so unioning them only ever adds the
/// second branch's own genuinely-distinct obligatory rewrite(s), never a "nothing happened"
/// alternative.
#[allow(clippy::too_many_arguments)]
fn compile_rtl_branch_net(
    opts: &FomaOptions,
    alphabet: &SegAlphabet,
    dir: Dir,
    lhs_slots: &[Slot],
    rhs_slots: &[Slot],
    left_slots: &[Slot],
    right_slots: &[Slot],
    asg: &AlphaAssignment,
    budget: &ComposeBudget,
    rule_xml_id: &str,
) -> Result<Fsm, ComposeError> {
    let plain_regex =
        render_branch_regex(alphabet, lhs_slots, rhs_slots, left_slots, right_slots, asg);
    let plain_net = fsm_parse_regex(opts, &plain_regex, None, None).unwrap_or_else(|| {
        panic!("foma rejected compiled regex for rule {rule_xml_id}: {plain_regex:?}")
    });
    match dir {
        Dir::LeftToRight => Ok(plain_net),
        Dir::RightToLeft => {
            let mirror_lhs = reversed_slots(lhs_slots);
            let mirror_rhs = reversed_slots(rhs_slots);
            // Swap: the mirror rule's own left environment is the REVERSED original right
            // environment, and vice versa (module doc).
            let mirror_left = reversed_slots(right_slots);
            let mirror_right = reversed_slots(left_slots);
            let mirror_regex = render_branch_regex(
                alphabet,
                &mirror_lhs,
                &mirror_rhs,
                &mirror_left,
                &mirror_right,
                asg,
            );
            let mirror_net = fsm_parse_regex(opts, &mirror_regex, None, None).unwrap_or_else(|| {
                panic!(
                    "foma rejected compiled mirror-rule regex for rule {rule_xml_id}: \
                     {mirror_regex:?}"
                )
            });
            let reversed_net = fsm_reverse(mirror_net);
            union_checked(
                opts,
                plain_net,
                reversed_net,
                budget,
                "compile_rtl_branch_net safety-net union",
            )
        }
    }
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
///
/// **Mode/dir detection (Phase C, `docs/fst-plan/phase-c-generator-design.md` §5/§6):**
/// `rule.mode`/`rule.dir` are checked FIRST, via [`is_fully_supported_shape`] -- a rule outside
/// that shape returns `Ok(None)` immediately, exactly the same "uncovered, caller reports it
/// `skipped`" contract [`pattern_slots`] already uses for an unsupported PATTERN construct
/// (Quantifier/Segments/Anchor). Before this check existed, an unsupported mode/dir was silently
/// compiled via plain foma `->` as if it were Iterative/LeftToRight -- a WRONG network with no
/// signal (design doc §5's "SILENT MIS-MAP" row). `Dir::RightToLeft` used to be gated out here too
/// (`Ok(None)`, honestly skipped) until `openspec/changes/compile-right-to-left-rewrites` gave it
/// real semantics ([`compile_rtl_branch_net`], module doc) -- both `Iterative` directions now
/// compile unconditionally. `RewriteMode::Simultaneous` used to be gated out here UNCONDITIONALLY
/// too, until `openspec/changes/compile-simultaneous-rewrites` gave `is_fully_supported_shape` a
/// per-rule admission check for it (that function's own doc) -- a `Simultaneous` rule whose
/// subrules the `simultaneous.subrule-overlap` predicate (D3) proves pairwise non-overlapping now
/// compiles via this SAME sequential-compose loop, unmodified; one the predicate cannot clear
/// stays gated here exactly as before. Every reference-grammar rule (Indonesian/Amharic/Sena) is
/// already `Iterative`/`LeftToRight` (this function's own prior module-level doc), so none of
/// these three changes alters any existing grammar's compiled output -- verified by
/// `tests/p6_gate_parity.rs`'s byte-exact Amharic state/arc-count regression guard and
/// `tests/f3_parity.rs`'s multiset parity gates staying green.
pub fn compile_rewrite_rule_subset(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    rule: &RewriteRuleDef,
    allowed: &dyn Fn(usize) -> bool,
    budget: &ComposeBudget,
) -> Result<Option<(Fsm, Vec<TupleReport>)>, ComposeError> {
    if !is_fully_supported_shape(g, rule) {
        return Ok(None);
    }
    // `openspec/changes/fix-multitable-fst-compilation`: resolved ONCE per rule (LHS is shared
    // across every subrule, module doc), never re-derived per subrule/slot and never an implicit
    // `g.char_tables[0]` default -- see [`owning_table`]'s own doc for how it finds the rule's
    // owning stratum. `None` (rule not wired into any stratum's own cascade at all) is treated
    // exactly like an unsupported pattern construct -- uncovered, reported `skipped` by this
    // function's own callers, never a silent table-zero guess.
    let Some(table) = owning_table(g, rule) else {
        return Ok(None);
    };
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
        let Some(lhs_slots) = pattern_slots(g, table, &rule.lhs, &mut next_occurrence) else {
            return Ok(None);
        };
        let Some(rhs_slots) = pattern_slots(g, table, &subrule.rhs, &mut next_occurrence) else {
            return Ok(None);
        };
        let left_slots = match &subrule.left_env {
            Some(p) => match pattern_slots(g, table, p, &mut next_occurrence) {
                Some(s) => s,
                None => return Ok(None),
            },
            None => Vec::new(),
        };
        let right_slots = match &subrule.right_env {
            Some(p) => match pattern_slots(g, table, p, &mut next_occurrence) {
                Some(s) => s,
                None => return Ok(None),
            },
            None => Vec::new(),
        };

        let (assignments, report) = resolve_alpha_tuples(
            table,
            &[
                lhs_slots.as_slice(),
                rhs_slots.as_slice(),
                left_slots.as_slice(),
                right_slots.as_slice(),
            ],
        );
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
            let branch_net = compile_rtl_branch_net(
                opts,
                alphabet,
                rule.dir,
                &lhs_slots,
                &rhs_slots,
                &left_slots,
                &right_slots,
                asg,
                budget,
                &rule.xml_id,
            )?;
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
        // Direction/mode fidelity (module doc): every reference-grammar rule this prototype has
        // seen is `Iterative`/`LeftToRight`, compiled via plain foma `->` (unioned per alpha-tuple,
        // see [`compile_rewrite_rule`]'s doc). A `RightToLeft` or `Simultaneous` rule is honestly
        // reported `skipped` instead -- [`compile_rewrite_rule_subset`]'s own `is_fully_supported_
        // shape` check (Phase C, that function's doc) makes this a detected, reported gap rather
        // than a silent mis-map.
        match compile_rewrite_rule_subset(opts, g, alphabet, rule, &|_| true, budget)? {
            Some((net, reports)) => {
                tuple_reports.push((rule.xml_id.clone(), reports));
                composed = Some(match composed {
                    None => net,
                    Some(prev) => compose_checked(
                        opts,
                        prev,
                        net,
                        budget,
                        "compile_and_compose_rules cascade fold",
                    )?,
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
        // See `compile_and_compose_rules_with_budget`'s own doc: mode/dir detection now lives in
        // `compile_rewrite_rule_subset` itself (`is_fully_supported_shape`), so an unsupported
        // shape is reported `skipped` for every group, never silently mis-compiled.
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

/// `true` iff `rule.mode` (and, for `Simultaneous`, `rule`'s own subrule shape against `g`) is a
/// shape this file's compile functions claim fidelity for. `RewriteMode::Iterative` compiles under
/// EITHER `Dir` (`Dir::LeftToRight` via the plain construction; `Dir::RightToLeft` via
/// [`compile_rtl_branch_net`]'s reversal-plus-safety-net-union construction, `openspec/changes/
/// compile-right-to-left-rewrites`), unconditionally in-shape regardless of subrule content.
///
/// `RewriteMode::Simultaneous` (`openspec/changes/compile-simultaneous-rewrites`; ADR 0001, `docs/
/// adr/0001-honest-capability-boundary.md`, the "simultaneous rewrite" worked example): NOT
/// wholesale in/out of shape the way `Iterative`/`RightToLeft` are -- admitted *unless* two of
/// `rule`'s own subrules' environments can match at the same input position
/// (`crate::capability::simultaneous_rule_admitted_for_compile`, the SAME `simultaneous.subrule-
/// overlap` proof (D3) the capability GATE's own `SimultaneousSubruleOverlapPredicate` uses — one
/// shared algorithm, two call sites, so the gate and this compiler can never disagree). When
/// admitted, this file's EXISTING plain/iterative sequential-compose machinery
/// (`compile_rewrite_rule_subset`'s own per-subrule `fsm_compose` fold, unchanged code) is used
/// as-is: the admitted case's own defining property is that simultaneous application == sequential
/// application at every position (no two subrules can ever contest the same focus), so reusing
/// that machinery is not an approximation, it is the correct construction. A rule the predicate
/// cannot prove non-overlapping for (or with a self-opaquing subrule, or an unsupported pattern
/// node in a lowered span) stays OUTSIDE this shape -- `compile_rewrite_rule_subset` returns
/// `Ok(None)` for it exactly like any other unsupported construct, honest-unsupported, never a
/// wrong compile.
pub fn is_fully_supported_shape(g: &Grammar, rule: &RewriteRuleDef) -> bool {
    match rule.mode {
        RewriteMode::Iterative => true,
        RewriteMode::Simultaneous => crate::capability::simultaneous_rule_admitted_for_compile(g, rule).is_ok(),
    }
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
        pg_grammar::load(SYNTH_ALPHA_XML).unwrap_or_else(|e| {
            panic!("failed to load synthetic alpha fixture: {e}\n{SYNTH_ALPHA_XML}")
        })
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

        let budget =
            ComposeBudget::with_caps(usize::MAX, usize::MAX, 3, usize::MAX, usize::MAX, None);
        let err = compile_rewrite_rule_subset(&opts, &g, &alphabet, rule, &|_| true, &budget)
            .expect_err("6 surviving tuples must exceed a tuple_cap of 3");
        match err {
            ComposeError::AlphaTupleBudgetExceeded {
                surviving,
                limit,
                rule_xml_id,
            } => {
                assert_eq!(
                    surviving, 6,
                    "synthetic fixture's ncBig class must have exactly 6 members"
                );
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
        let budget =
            ComposeBudget::with_caps(0, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None);
        let err = compile_rewrite_rule_subset(&opts, &g, &alphabet, rule, &|_| true, &budget)
            .expect_err("composing 6 branch nets must exceed a state_cap of 0");
        assert!(
            matches!(
                err,
                ComposeError::NetSizeExceeded {
                    measure: NetSizeMeasure::States,
                    ..
                }
            ),
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
        let (net, reports) =
            compile_rewrite_rule_subset(&opts, &g, &alphabet, rule, &|_| true, &budget)
                .expect("unbounded budget must never trip")
                .expect("synthetic rule must compile");
        assert!(net.statecount > 0);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].surviving, 6);
    }
}

/// `openspec/changes/fix-multitable-fst-compilation` task 1.1's positive witness: a synthetic,
/// delanguaged "two-table-symbol-divergence" fixture -- two `<CharacterDefinitionTable>`s with
/// DIFFERENT segment counts (2 vs 3), two strata (each owning one of the tables), and an
/// alpha-bound `Simultaneous`-free rewrite rule on the SECOND stratum whose RHS natural class
/// (`ncBig`, an "Any"-style `FeatureNaturalClass` with zero explicit `FeatureValue` constraints)
/// matches EVERY segment of whichever table it is resolved against. The two tables' differing
/// CARDINALITY (not just differing feature-to-index alignment, `tests/phase_c_multi_table.rs`'s
/// own mechanism) makes this a doubly-independent proof: [`resolve_alpha_tuples`]'s own
/// `surviving` tuple count is a DIRECT, deterministic readout of WHICH table `ncBig` resolved
/// against (2 members if table 0, 3 if table 1) -- table 0's cardinality is the exact wrong answer
/// the old hardcoded `let table = &g.char_tables[0]` default would have produced.
#[cfg(test)]
mod owning_table_tests {
    use super::*;
    use pg_grammar::model::PhonRuleDef;

    /// Table 0 ("t0", stratum "S0"): 2 segments. Table 1 ("t1", stratum "S1"): 3 segments --
    /// deliberately a DIFFERENT cardinality from table 0 (module doc), so a rule resolving `ncBig`
    /// against the wrong table produces a DIFFERENT, wrong `surviving` count, not merely a
    /// same-count coincidentally-plausible one. `prule_alpha_t1` belongs to stratum "S1" (table
    /// "t1") via `phonologicalRules="prule_alpha_t1"`; stratum "S0" carries no rule of its own --
    /// it exists purely so this grammar genuinely has TWO strata each owning ITS OWN table, the
    /// design.md scenario ("two strata... tables"), not just two orphaned tables.
    const TWO_TABLE_ALPHA_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>TwoTableSymbolDivergenceAlphaFixture</Name>
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
    <CharacterDefinitionTable id="t0">
      <Name>Table0</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c0a"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
        <SegmentDefinition id="c0b"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <CharacterDefinitionTable id="t1">
      <Name>Table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1a"><Representations><Representation>k</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
        <SegmentDefinition id="c1b"><Representations><Representation>g</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
        <SegmentDefinition id="c1c"><Representations><Representation>x</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncBig"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule_alpha_t1">
        <Name>alpha rule on table 1</Name>
        <VariableFeatures>
          <VariableFeature id="var1" name="a" phonologicalFeature="featA" />
        </VariableFeatures>
        <PhoneticInput>
          <PhoneticSequence>
            <Segment segment="c1a" />
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
      <Stratum characterDefinitionTable="t0" morphologicalRuleOrder="unordered">
        <Name>S0</Name>
        <LexicalEntries>
          <LexicalEntry id="entry0" partOfSpeech="posV">
            <Allomorphs><Allomorph id="allo0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>dummy0</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prule_alpha_t1">
        <Name>S1</Name>
        <LexicalEntries>
          <LexicalEntry id="entry1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="allo1"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>dummy1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    fn two_table_alpha_grammar() -> Grammar {
        pg_grammar::load(TWO_TABLE_ALPHA_XML).unwrap_or_else(|e| {
            panic!("failed to load two-table-symbol-divergence alpha fixture: {e}\n{TWO_TABLE_ALPHA_XML}")
        })
    }

    fn rewrite_rule_by_xml_id<'g>(g: &'g Grammar, xml_id: &str) -> &'g RewriteRuleDef {
        for pr in &g.prules {
            if let PhonRuleDef::Rewrite(r) = pr {
                if r.xml_id == xml_id {
                    return r;
                }
            }
        }
        panic!("prule {xml_id:?} not found in g.prules");
    }

    /// Positive witness (task 1.1): [`owning_table`] resolves `prule_alpha_t1` to table 1 (3
    /// segments), never table 0 (2 segments) -- the fixture's own sanity check that the two
    /// tables genuinely differ in cardinality, and that stratum "S1" (not "S0") owns this rule.
    #[test]
    fn owning_table_resolves_to_the_rules_own_stratum_table_not_table_zero() {
        let g = two_table_alpha_grammar();
        assert_eq!(g.char_tables.len(), 2, "fixture must declare exactly 2 tables");
        assert_eq!(g.char_tables[0].len(), 2, "table 0 must have exactly 2 segments");
        assert_eq!(g.char_tables[1].len(), 3, "table 1 must have exactly 3 segments");
        assert_eq!(g.strata.len(), 2, "fixture must declare exactly 2 strata");

        let rule = rewrite_rule_by_xml_id(&g, "prule_alpha_t1");
        let table = owning_table(&g, rule)
            .expect("prule_alpha_t1 is wired into stratum S1's own phonologicalRules cascade");
        assert_eq!(
            table.len(),
            3,
            "prule_alpha_t1 belongs to stratum S1 (table 1, 3 segments) -- owning_table must NOT \
             return table 0's 2-segment table"
        );
    }

    /// Positive witness (task 1.1), full compile-level proof: [`resolve_alpha_tuples`]'s own
    /// `surviving` tuple count for `prule_alpha_t1`'s alpha-bound RHS (`ncBig`, matches every
    /// segment of WHICHEVER table it resolves against) is EXACTLY 3 -- table 1's own cardinality,
    /// reached by resolving against table 1 (this rule's real owning table, via [`owning_table`]),
    /// never table 0's 2 (what the old hardcoded `g.char_tables[0]` default would have produced --
    /// the NEGATIVE case this witness rules out).
    #[test]
    fn resolve_alpha_tuples_surviving_count_reflects_the_owning_table_not_table_zero() {
        let g = two_table_alpha_grammar();
        let rule = rewrite_rule_by_xml_id(&g, "prule_alpha_t1");
        let table = owning_table(&g, rule).expect("prule_alpha_t1 has an owning stratum");
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();
        let budget = ComposeBudget::unbounded();

        let (net, reports) =
            compile_rewrite_rule_subset(&opts, &g, &alphabet, rule, &|_| true, &budget)
                .expect("unbounded budget must never trip")
                .expect("prule_alpha_t1 must compile");
        assert!(net.statecount > 0);
        assert_eq!(reports.len(), 1, "exactly one alpha-bearing subrule");
        assert_eq!(
            reports[0].surviving, 3,
            "surviving tuple count must equal table 1's own 3-member ncBig class -- 2 would mean \
             this rule wrongly resolved against table 0 instead of its own stratum's table"
        );
        assert_eq!(
            reports[0].raw_product, 3,
            "a single alpha occurrence's raw product equals its own candidate set size"
        );
    }
}
