//! Compiles HC `RewriteRuleDef`s into foma replace-calculus regex source (`A -> B || L _ R`).
//!
//! This is the relational encoding of a rewrite rule: the rule stays a relation rather than being
//! expanded into every surface junction variant at build time the way `crate::junctions` and
//! `crate::preexpand` do.
//!
//! ## Symbol alphabet: char-def IDENTITY, not literal spelling
//! The engine matches phonological segments by **char-def identity**, never by literal spelling
//! (`emit.rs`'s module doc, "Surface spelling": a char-def with several `<Representation>`s
//! matches ANY of its own spellings). `emit.rs` copes with this by cartesian-producting every
//! spelling variant into literal lexc strings (`crate::emit::surface_variants`). This module
//! takes the more direct route available once lexc/rules are built from `pg_shape::Shape`
//! structure rather than raw text: every `CharDefId` used anywhere in the grammar's surface
//! table is mapped to **one Private-Use-Area codepoint** (`SegAlphabet::token`), and every lexc
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
//! fine for the propose→confirm contract: `crate::analyzer::FomaProposer`-equivalent callers only need
//! the UPPER tape's tag sequence; a query word is transliterated into token space
//! (`SegAlphabet::encode_query`, reusing `pg_grammar::segment::segment_phonemes_only` — the
//! same greedy longest-match the engine's own segmentation uses) before `apply_up`, and the
//! result is decoded via `crate::tags::decode_path` exactly like the mainline proposer.
//!
//! ## alpha-variable expansion: tuple-indexed, not per-variable
//! A rule's alpha-bound slots (RHS/LHS/environment `pg_grammar::model::PatternNode::Context` nodes carrying
//! `pg_grammar::model::AlphaVar`s) are resolved by `resolve_alpha_tuples`: gather every slot referencing a given
//! `pg_grammar::model::VarId`, enumerate the CROSS PRODUCT of each slot's own (non-alpha-feature) candidate
//! members, then keep only the combinations where every pair of same-`VarId` slots satisfies its
//! own joint `AlphaVar::plus` polarity: `+`/`+` or `-`/`-` must overlap (same symbolic-feature
//! value) at that variable's lane, `+`/`-` must be disjoint (a different value). This bounds the
//! count of segment tuples
//! satisfying the joint constraint (Amharic's 20-variable CV-merger: nc15=59 × nc16=6 ⇒ ≤354, never
//! v^20) — implemented once, generically over N variables and N slots-per-variable, so the same
//! code path that resolves Indonesian's single-variable prule4 is what would resolve Amharic's
//! rule without modification.
//!
//! A disagree-polarity occurrence is refused instead when its own natural class does not make
//! disagreement a FUNCTION -- `crate::lower::class_feature_partition_is_unambiguous`'s own doc has
//! the reason (a genuine one-to-many relation this branch-union construction was measured
//! collapsing to a single, often wrong, branch).
//!
//! ## What this module does NOT attempt
//! - `pg_grammar::model::PatternNode::Quantifier` (`OptionalSegmentSequence`) that is inverted (`min > max`, `max`
//!   concrete), empty, or carries an alpha-bound occurrence anywhere in its own children —
//!   `pattern_slots` still returns `None`/bails for exactly these configurations (a rule whose
//!   pattern needs one is reported uncovered, not silently mis-rendered). A FINITELY bounded,
//!   alpha-free quantifier (`min`/`max` both concrete, `min <= max`) compiles via `Slot::Repeat`,
//!   and a genuinely UNBOUNDED, alpha-free quantifier (`max ==
//!   None`, the DTD's `max="-1"` sentinel) now ALSO compiles, via that SAME `Slot::Repeat`
//!   (`max: Option<u32>`), rendered with foma's native `E*`/`E^>N` operator instead of `E^{min,max}`
//!   — see that variant's own doc for the construction, and "Bounded quantifiers" below for the
//!   compiled-vs-still-unsupported line and the confirm-engine finding that motivates it.
//! - `RewriteMode::Simultaneous` whose subrules the `simultaneous.subrule-overlap` predicate
//!   (`crate::capability`) cannot prove pairwise non-overlapping (self-opaquing, an unresolved
//!   overlap, or an unsupported pattern node in a lowered span) — see "`RewriteMode::Simultaneous`:
//!   compiling the ADMITTED case" below for the (now real) admitted case.
//! - MPR gating (`required_mpr`/`excluded_mpr` on a subrule) — flag-diacritic emission is
//!   out of scope, not attempted in this slice.
//!
//! ## `Dir::RightToLeft`: the reversal construction
//! `Dir::RightToLeft` used to be honestly skipped (the same `None` treatment `Simultaneous`
//! still gets); this change gives it real, direction-faithful semantics via the STANDARD
//! finite-state technique for "prefer the rightmost, not leftmost, non-overlapping match" (Beesley
//! & Karttunen, *Finite State Morphology*, ch. 6 "Directional replacement rules"): reverse ∘
//! compile(mirror rule) ∘ reverse, NOT "compile as if `LeftToRight`".
//!
//! **The mirror rule.** Foma's native `->` only ever prefers the LEFTMOST of several
//! non-overlapping candidate matches (there is no built-in "prefer rightmost" operator). To get
//! rightmost preference, `compile_rtl_branch_net` builds the MIRROR IMAGE of the rule — reverse
//! the LHS's own slot order, reverse the RHS's own slot order, and SWAP the two environments while
//! ALSO reversing each one's own slot order (`left_env' = reverse(right_env)`, `right_env' =
//! reverse(left_env)`) — compiles that mirror rule with the SAME plain-`->` machinery
//! `render_branch_regex` already uses for `LeftToRight`, and then calls `fsm_reverse` on the
//! resulting `Fsm`. `fsm_reverse`'s own contract (`foma::reverse`'s doc: "all original state
//! numbers are shifted up by 1... label sides are NOT swapped") means: for a transducer whose own
//! upper/lower tapes spell `reverse(S)`/`reverse(S')` when read forward, `fsm_reverse` of it spells
//! `S`/`S'` when read forward — i.e. reversing a network that operates on REVERSED strings gives
//! back a network that operates on NORMAL strings, but the internal left-to-right preference that
//! was baked into the mirror compile (over the reversed alphabet) becomes a right-to-left
//! preference over the real, un-reversed string. Environments keep their ordinary, un-reversed
//! meaning in the FINAL network (`left_env` is still "precedes the target in the real string") —
//! the swap+reverse only happens in the INTERMEDIATE mirror-rule text; see
//! `compile_rtl_branch_net`'s own doc for the worked "aa -> b" example this construction is
//! checked against.
//!
//! **The safety-net union (a documented, conservative judgment call).** `pg_rules::rewrite`'s own
//! `Iterative` synthesis/analysis loops (`syn_feature`/`syn_narrow`/`ana_feature`/…) pick which
//! candidate span to act on first via `all_spans`'/`candidates.sort_unstable()`'s own ASCENDING
//! sort — i.e. this repo's current full-HC oracle is, empirically, direction-BLIND for the "which
//! overlapping match wins" question (verified directly: a hand-built `aa -> b` rule applied to
//! `"aaa"` synthesizes to `"ba"` whether the rule is declared `LeftToRight` or
//! `rightToLeftIterative`). Where the oracle itself is unverified for a configuration, the
//! configuration is unsupported by definition. Rather than let a THEORETICALLY-faithful
//! reversal-only compile under-propose relative to what this repo's own confirm engine actually
//! requires for recall (the reversal-only net for `aa -> b`/`RightToLeft` maps `"aaa"` to `"ab"`,
//! never `"ba"` — so it would never even PROPOSE the lexical form the current oracle confirms for
//! surface `"ba"`), `compile_rtl_branch_net` returns `fsm_union(plain_LTR_style_net,
//! reversed_net)`: the SAME plain construction `render_branch_regex` already gives `LeftToRight`
//! (a proven-safe floor, since the oracle treats every direction identically today) UNIONED with
//! the genuinely-reversed net (so the construction really is direction-aware, differs from a plain
//! `LeftToRight` compile on any input where the two branches disagree, and is READY the day
//! `pg_rules::rewrite`'s own pick-order gets a direction-aware fix — a follow-on outside this
//! single-owner file's scope, flagged, not fixed here). Both branches are already COMPLETE,
//! obligatory replace transducers (each has no "did nothing" identity path at a position its own
//! context matches), so `fsm_union`ing them adds no spurious third "nothing happened" path — see
//! `compile_rtl_branch_net`'s own doc for why this differs from the alpha-tuple union-is-wrong
//! finding above.
//!
//! ## `RewriteMode::Simultaneous`: compiling the ADMITTED case
//! `RewriteMode::Simultaneous` used to be honestly skipped UNCONDITIONALLY (`None` for every
//! such rule, regardless of subrule shape — the same treatment metathesis and an unsupported
//! pattern construct get). It still stays that way for a rule whose subrules the
//! `simultaneous.subrule-overlap` predicate (`crate::capability::
//! SimultaneousSubruleOverlapPredicate`) cannot prove pairwise
//! non-overlapping. What changes here: for a rule the predicate DOES admit —
//! `is_fully_supported_shape` now asks `crate::capability::
//! simultaneous_rule_admitted_for_compile` (that function's own doc: the SAME proof, freshly
//! computed, sharing its algorithm with the capability gate's own predicate so the two can never
//! disagree) — this file's EXISTING plain/iterative sequential-compose machinery is reused UNCHANGED,
//! not reimplemented: no new branch net construction, no new fold shape, nothing analogous to
//! `compile_rtl_branch_net`'s mirror-plus-reverse-plus-union.
//!
//! **Why reuse, not a new algorithm, is actually correct here (not merely convenient).** The
//! Admit boundary is defined EXACTLY as "no two subrules' environments can ever match at the same
//! input position" — precisely the condition under which HC's true `Simultaneous` semantics (find
//! every match against ONE untouched input snapshot, then apply them all —
//! `SimultaneousPhonologicalPatternRule.Apply`, HC's own reference behavior) and a sequential
//! per-subrule fold (this file's existing `Iterative`-labeled machinery) produce IDENTICAL output:
//! with no shared focus position in contention, subrule application order can never change which
//! subrule wins where, so "compose subrule 1's net, then subrule 2's net" (what this file already
//! does for `Iterative`) and "collect all subrules' matches against the original input, then apply
//! all of them" (true `Simultaneous`) coincide. A second, independently-confirmed reason this
//! reuse is faithful, not just permitted: a plain foma `->` replace rule is ITSELF a single-pass,
//! snapshot-style construction (Beesley & Karttunen's classical replace-rule automaton finds every
//! non-overlapping match against the rule's own input tape and rewrites them all in one
//! transduction — it cannot self-feed within one compiled expression the way `pg-rules`'
//! `syn_feature`'s re-scan-after-every-mutation loop can (`syn_epenthesis` is "already
//! Simultaneous-shaped" for exactly this reason). So
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
//! `self_opaquing` — a real, load-bearing mode
//! dependence, ported and shipped from HC's own reference behavior, not a gap this change needs to
//! patch around. The
//! `self_opaquing`-Refuse early-out is exactly what keeps the ADMITTED case inside the region where
//! this asymmetry never actually bites: `self_opaquing` is REQUIRED true for the repeat-wrapper to
//! ever trigger, and the admit predicate refuses any pair containing one
//! (`crate::capability::simultaneous_rule_admitted_for_compile` is additionally stricter still for
//! a LONE self-opaquing subrule, unlike the predicate's own pairwise-only algorithm). So
//! for every
//! rule this file now actually compiles under `Simultaneous`, confirm's analysis side runs
//! `ana_feature`/`ana_epenthesis` exactly once, per subrule, with no fixpoint loop — the SAME shape
//! `Iterative` mode's analysis already uses ("`ApplicationMode`
//! has zero effect on which pattern rule analysis uses" for Feature subrules, HC's own reference
//! behavior). No safety-net union
//! is needed here (contrast `compile_rtl_branch_net`'s own documented judgment call): there is no
//! known faithfulness gap between what this file compiles and what confirm accepts for the admitted
//! case, so no superset-widening is required to stay recall-safe.
//!
//! ## Bounded quantifiers
//! `pg_grammar::model::PatternNode::Quantifier` (`<OptionalSegmentSequence min max>`) used to be `pattern_slots`'
//! unconditional bail (module doc, "What this module does NOT attempt") regardless of `min`/`max`.
//! Now a FINITELY bounded, alpha-free quantifier — `max == Some(_)`, `min <= max`, no `Slot::Alpha`
//! occurrence anywhere in its own (possibly nested)
//! children — compiles to a new `Slot::Repeat`, rendered as foma's OWN native bounded-repetition
//! xre operator, `A^{min,max}` (`nfst-xre = "0.1.0"`'s `RepeatNToK`, confirmed by reading that
//! vendored crate's own `src/lexer.rs`/`src/parser.rs`: `^{N,K}`/`^N,K` lexes to `CatenateNToK`, a
//! POSTFIX operator over whatever `[...]`-grouped term precedes it) over the quantifier's own
//! rendered children — never a hand-rolled state-machine construction, so this file inherits
//! foma's own `fsm_concat_m_n` construction (`foma = "0.4.0"`'s own `src/constructions/boolean.rs`:
//! `min` mandatory concatenated copies of the child net, then `max - min` further copies each
//! wrapped in `fsm_optionality` — i.e. **exactly** the "bounded concatenation/optionality"
//! construction this change's own proposal names, not an approximation of it) for free. Inverted
//! (`min > max`, `max` concrete, no sound finite construction), empty-children, or alpha-nested
//! quantifiers are UNCHANGED: still `None`, still honestly reported uncovered by every existing
//! caller.
//!
//! ## Unbounded quantifiers
//! A genuinely UNBOUNDED, alpha-free quantifier — `max == None`, the DTD's `max="-1"` Kleene
//! sentinel, the loader's own DEFAULT when the attribute is absent (`XmlLanguageLoader.cs`, the
//! DTD's own `#IMPLIED` doc: "-1 or higher") — used to be refused for exactly the same reason a
//! bounded one used to be: `pattern_slots`' unconditional bail, inherited from bounded quantifier
//! support's own narrower original scope, never a feasibility finding (the unbounded case was
//! never uncompilable, only out of scope for that first step). It compiles now, via the SAME `Slot::Repeat` (widened to
//! `max: Option<u32>`), rendered as foma's own native `E*`/`E^>N` xre operator instead of
//! `E^{min,max}` (`crate::lower::render_slots`'s own doc has the exact operator-selection rule):
//! `min == 0` ("zero or more") is plain `*` (`nfst-xre`'s `Token::Star`, `foma-0.4.2`'s
//! `UnaryOp::Star` -> `fsm_kleene_star`); `min >= 1` ("`min` or more") is `E^>(min-1)`
//! (`nfst-xre`'s `CatenateNPlus`/`RepeatNPlus`, `foma-0.4.2/src/regex.rs:258-268`'s own
//! `concat(concat_n(net, N), kleene_plus(net))` — **`E^>N` means MORE THAN `N`, i.e. `N+1` or more,
//! not `N` or more**, the off-by-one `crate::lower::render_slots` is careful to get right by
//! rendering `min-1`, never `min`). A Kleene star/plus's own compiled net size does not depend on
//! any repetition count at all, and `max: None` is never coerced to a concrete number anywhere in
//! this path (a finite cutoff must never masquerade as unbounded semantics — this is the SAME rule
//! the original refusal existed to enforce, now honored by actually building the unbounded
//! construction instead of refusing every quantifier that might need it). Inverted-,
//! empty-children, and alpha-nested quantifiers stay `None` exactly as before.
//!
//! **Big-O.** `E*`/`E^>N`'s compiled size is `fsm_kleene_star`/`fsm_kleene_plus`'s own native
//! construction over the child automaton `E` (a small, constant number of extra states/arcs beyond
//! `E` itself, `N` sequential copies of `E` for the `E^>N` case's own mandatory prefix) — LINEAR in
//! `min`, and, unlike the finite `E^{min,max}` case, INDEPENDENT of any upper occurrence count (there
//! is none to be linear or exponential IN).
//!
//! **Big-O.** `A^{min,max}`'s compiled size is `O(max · |A|)` states/arcs (`max` sequential copies
//! of the child automaton `A`, `fsm_concat_m_n`'s own doc above) — LINEAR in the bound, never
//! exponential, and independent of `min` (a smaller `min` only changes how many of the `max` copies
//! are wrapped `fsm_optionality`-skippable, not how many copies exist). A rule combining a
//! quantifier with alpha variables ELSEWHERE in the same subrule (never inside the quantifier's own
//! children — disallowed, see `Slot::Repeat`'s own doc) multiplies this bound by
//! `resolve_alpha_tuples`'s own `surviving` tuple count, exactly the same two-independent-axes
//! shape; the quantifier axis gets its OWN eager, cheaper-than-any-
//! `Fsm` characterization rather than a new composition-budget dimension: `pattern_slots` is a pure
//! structural walk with no `ComposeBudget` threaded through it (every existing caller — this file's
//! own compile path, `crate::lower::lower_span`, `crate::capability`'s structural probes — calls it
//! with only a `&Grammar`/`&CharDefTable`), and widening that signature crate-wide for one
//! dimension's sake was judged a larger, separate follow-on rather than something this single-owner
//! slice should take on.
//!
//! **Confirm-engine finding (recall RTL's own "recall this can have gaps" note): a Quantifier whose
//! own occurrence count can make it match a PHYSICAL WIDTH other than exactly 1 segment, used as (or
//! inside) a rule's LHS/RHS focus, cannot be confirmed by `pg_rules::rewrite` at all today** —
//! `pg_rules::rewrite::width_matches`'s own doc (`rewrite.rs`, "Shared width-mismatch guard")
//! requires the ACTUAL matched span width to equal the rule's raw `lhs.nodes.len()`
//! (`Kind::Narrow`) or `rhs.nodes.len()` (`Kind::Feature`) — a plain node COUNT that is always
//! exactly 1 for "one `Quantifier` node occupies the entire LHS", regardless of how many physical
//! segments it actually consumes; any occurrence count whose real width differs from that fixed
//! count (e.g. `max > 1`, or `min == 0`'s zero-occurrence skip) is silently discarded by this guard
//! before the RHS is ever applied, INDEPENDENT of this change (`width_matches` predates it; the
//! guard's own doc explains it exists for a DIFFERENT, unrelated scenario — an earlier rule's own
//! analysis-inserted Optional segment widening a LATER rule's match span — that merely also catches
//! this one). **A `Quantifier` used inside a rule's `left_env`/`right_env` has no such gap**:
//! `pg_rules::rewrite::left_env_match`/`right_env_match` compile the environment via the SAME
//! `PatternBridge::compile_pattern` bridge this crate's own oracle-comparison tests already rely on
//! being Quantifier-faithful (`pg-rules/src/bridge.rs`'s own doc: "the pg-fst `{min,max}` quantifier
//! over the compiled children"), and test only FIRST-MATCH EXISTENCE (`Transduce::first_match`), never
//! a positional per-node array — no width count to mismatch. `tests/phase_c_quantifier.rs`'s own
//! bounded-quantifier containment fixture therefore places its quantifier in a `right_env`
//! (`prule3`'s own precedent this module's earlier doc already cited), where exact oracle
//! containment is provable today; a genuinely LHS/RHS-focus-quantified rule is real, compilable
//! FST-side, but its full-recall containment against `pg_rules::rewrite` is a documented, pre-
//! existing gap this change surfaces rather than silently works around — flagged for a follow-on
//! entirely outside `replace.rs`'s single-owner boundary, exactly like the RTL gap above.
//!
//! ## Additional `RightToLeftRewrite` pattern shapes
//! `pattern_slots` used to refuse `PatternNode::Segments`/`PatternNode::Anchor`
//! unconditionally, for EVERY caller alike — the RTL predicate's own witness used to list them
//! alongside a malformed `Quantifier`/a disagree-polarity alpha var as the shapes
//! `compile_rtl_branch_net` excludes; the latter is admitted too now (below). Re-examining each
//! one at the reversal construction's own level (`crate::lower::
//! PatternLowerScope`'s own doc has the full per-consumer boundary this section only summarizes):
//! - **`Segments` (same or different table).** Same-table literals lower to ordinary
//!   `crate::lower::Slot::Fixed` atoms. Cross-table literals lower to table-qualified
//!   `crate::lower::Slot::ForeignFixed` atoms and render as the union of owning-table tokens whose
//!   feature lanes unify with the foreign segment, matching the oracle without reinterpreting raw
//!   ids across tables. Both remain atomic under reversal.
//! - **`Anchor` (word-boundary condition).** Lowers to a new `crate::lower::Slot::Anchor`,
//!   rendered as foma's own `.#.` xre atom. The slot's position, not the source-side tag, conveys
//!   word-initial vs. word-final. This is exactly why the mirror-and-reverse construction swaps an
//!   anchor to the CORRECT opposite edge with ZERO new code in
//!   `compile_rtl_branch_net`/`reversed_slots` themselves: an anchor
//!   that is the LAST slot of the original `right_env` becomes, via the EXISTING `reversed_slots`
//!   (pure position reversal, no anchor-specific case) plus the EXISTING left/right swap, the FIRST
//!   slot of the mirror's own `left_env` — a leading `.#.` there means "start of the
//!   mirror/reversed representation", which `fsm_reverse` then correctly turns into "end of the
//!   real string" for the final network, by the SAME "reversing a network that operates on
//!   reversed strings gives back a network operating on normal strings" argument this file's own
//!   RTL section above already makes for ordinary content. Pinned empirically (not just argued):
//!   `tests/phase_c_right_to_left.rs`'s `rtl_anchor_reversal_swaps_the_correct_edge`.
//! - **A disagree-polarity alpha var** (`AlphaVar::plus == false`) now lowers to a `Slot::Alpha`
//!   like any other occurrence, unless its own class makes disagreement ambiguous
//!   (`crate::lower::class_feature_partition_is_unambiguous`) — this was always orthogonal to
//!   reversal (`resolve_alpha_tuples`' own joint-polarity filter, the SAME gap for an ordinary
//!   `LeftToRight` rule, not something the mirror-and-reverse construction has any bearing on
//!   either way), so admitting it here is exactly as safe under `Dir::RightToLeft` as under
//!   `Dir::LeftToRight`.
//! - **This widening is scope-gated** (`crate::lower::PatternLowerScope`), not a blanket change:
//!   `crate::lower::lower_span`'s own callers are unaffected, still passing
//!   `crate::lower::PatternLowerScope::Baseline`.

use foma::constructions::{fsm_compose, fsm_union, fsm_universal};
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::reverse::fsm_reverse;
use foma::types::Fsm;

use std::collections::HashMap;

use pg_featstruct::flat_unifiable;
use pg_grammar::chardef::{CharDefId, CharDefTable};
use pg_grammar::model::{
    Dir, Grammar, MetathesisRuleDef, PRuleId, PhonRuleDef, RewriteMode, RewriteRuleDef,
    RewriteSubruleDef, TableId,
};

/// Private-Use-Area base codepoint every `CharDefId` is offset from; no in-scope grammar has enough char-defs to overflow the PUA block.
const PUA_BASE: u32 = 0xE000;

/// Cross-table representation aliasing: a normalized
/// representation -> every `(TableId, CharDefId)` across the WHOLE grammar that spells it,
/// built once per grammar. **Same NFD normalization as `CharDefTable::lookup_nfd`/
/// `crate::emit::surface_variants`** — this reuses [`pg_grammar::chardef::CharDef::
/// representations_nfd`] directly (the exact keys `CharDefTable`'s own internal `lookup` map is
/// built from, `pg-grammar/src/chardef.rs`'s `from_raw`), never a second, independently-derived
/// normalization.
///
/// Mirrors `capability.rs`'s own `multi_table_detail` pairwise-disjointness check (same source
/// data, same normalization), but keeps every table's own contribution instead of only recording
/// whether an overlap exists at all — `Self::aliases_for` is the render-time consumer.
pub(crate) struct RepresentationAliasMap {
    by_repr: HashMap<String, Vec<(TableId, CharDefId)>>,
    by_feature_constraint: HashMap<(TableId, CharDefId), Vec<(TableId, CharDefId)>>,
}

impl RepresentationAliasMap {
    /// Cheap: `O(total char-defs across every table)`, a characterization-time cost identical in
    /// shape to `multi_table_detail`'s own pairwise scan (that function's own doc: "cheap for any
    /// grammar in scope — table counts are small"). Built fresh per call (not memoized across
    /// rules) — deliberately simple for now; alias-set size/rebuild cost is a separate
    /// question the design doc's own "What this design does NOT settle" section defers to future
    /// measurement on a real multi-table grammar, not something to guess a cache shape for here.
    pub(crate) fn build(g: &Grammar) -> Self {
        let mut by_repr: HashMap<String, Vec<(TableId, CharDefId)>> = HashMap::new();
        for (ti, table) in g.char_tables.iter().enumerate() {
            let table_id = TableId(ti as u16);
            for (cd_id, cd) in table.iter() {
                for rep in cd.representations_nfd() {
                    by_repr
                        .entry(rep.clone())
                        .or_default()
                        .push((table_id, cd_id));
                }
            }
        }
        let mut by_feature_constraint = HashMap::new();
        for (source_ti, source_table) in g.char_tables.iter().enumerate() {
            let source_table_id = TableId(source_ti as u16);
            for (source_cd, source_definition) in source_table.iter() {
                let mut compatible = Vec::new();
                for (target_ti, target_table) in g.char_tables.iter().enumerate() {
                    let target_table_id = TableId(target_ti as u16);
                    for (target_cd, target_definition) in target_table.iter() {
                        if flat_unifiable(
                            target_definition.feature_lanes(),
                            source_definition.feature_lanes(),
                        ) {
                            compatible.push((target_table_id, target_cd));
                        }
                    }
                }
                by_feature_constraint.insert((source_table_id, source_cd), compatible);
            }
        }
        RepresentationAliasMap {
            by_repr,
            by_feature_constraint,
        }
    }

    /// Every `(TableId, CharDefId)` sharing a normalized representation with `cd`, always including `(table_id, cd)` itself; never empty.
    fn aliases_for(
        &self,
        table: &CharDefTable,
        table_id: TableId,
        cd: CharDefId,
    ) -> Vec<(TableId, CharDefId)> {
        let mut out: Vec<(TableId, CharDefId)> = Vec::new();
        for rep in table.get(cd).representations_nfd() {
            if let Some(group) = self.by_repr.get(rep) {
                for &pair in group {
                    if !out.contains(&pair) {
                        out.push(pair);
                    }
                }
            }
        }
        if !out.contains(&(table_id, cd)) {
            // Defensive: guards against a future change to the map silently dropping the atom's own token.
            out.push((table_id, cd));
        }
        out
    }
    fn feature_constraint_aliases_for(
        &self,
        table: TableId,
        cd: CharDefId,
    ) -> &[(TableId, CharDefId)] {
        self.by_feature_constraint
            .get(&(table, cd))
            .map(Vec::as_slice)
            .expect("foreign constraint must name a character definition from this grammar")
    }
}

/// Maps `CharDefId`s to/from single Private-Use-Area codepoints (module doc). Cheap to
/// construct (`table` borrow only); one instance is shared across rule compilation, lexc
/// emission, and query encoding for one grammar/table pair.
pub struct SegAlphabet<'t> {
    table: &'t CharDefTable,
    /// Table identity plus alias map, always both or neither; `None` (via `Self::new`) means this alphabet was never asked to alias.
    aliasing: Option<(TableId, &'t RepresentationAliasMap)>,
}

impl<'t> SegAlphabet<'t> {
    pub fn new(table: &'t CharDefTable) -> Self {
        SegAlphabet {
            table,
            aliasing: None,
        }
    }

    /// The constructor the design mandates for a multi-table-aware, render-time-aliasing caller:
    /// takes `table` AND its own `table_id` together, never separately/defaulted (design item 2).
    /// The only production caller is `compile_rewrite_rule_subset`, which builds one of these
    /// per rule from `owning_table`/`owning_table_id`'s own resolution — never `g.char_tables
    /// [0]`.
    pub(crate) fn with_table_id(
        table: &'t CharDefTable,
        table_id: TableId,
        aliases: &'t RepresentationAliasMap,
    ) -> Self {
        SegAlphabet {
            table,
            aliasing: Some((table_id, aliases)),
        }
    }

    /// The single codepoint standing in for `cd` everywhere (lexc lower-tape text, rule regex
    /// atoms, encoded query words).
    pub fn token(&self, cd: CharDefId) -> char {
        char::from_u32(PUA_BASE + cd.0).expect("char table too large for the PUA token scheme")
    }

    /// Every token that should stand for `cd` when RENDERING a pattern atom (a rule's LHS/RHS/
    /// environment text — `crate::lower::render_slots`'s `Slot::Fixed`/`Slot::Union` arms,
    /// design item 3) — the render-time cross-table alias-expansion union. Exactly `[self.token
    /// (cd)]`, in the same order, whenever this alphabet carries no `Self::aliasing` (built via
    /// `Self::new`) OR `cd`'s own spelling happens to be unique to this table — i.e. byte-
    /// identical to the pre-aliasing behavior for every existing single-table grammar and every
    /// multi-table grammar with no shared representation, which is exactly why
    /// `tests/p6_gate_parity.rs`/`tests/f3_parity.rs` stay green (module doc, "no reference
    /// grammar is multi-table with shared representations"). When `cd`'s spelling IS shared with
    /// another table, returns the union over every `(table, cd')` sharing it (deduplicated,
    /// `RepresentationAliasMap::aliases_for`'s own contract) — this only ever ADDS alternatives,
    /// never removes `Self::token(cd)` itself, the design's own recall-safety argument.
    pub(crate) fn render_tokens(&self, cd: CharDefId) -> Vec<char> {
        match self.aliasing {
            None => vec![self.token(cd)],
            Some((table_id, aliases)) => {
                let mut chars: Vec<char> = aliases
                    .aliases_for(self.table, table_id, cd)
                    .into_iter()
                    .map(|(_tid, acd)| {
                        // Same PUA formula as this alphabet's own tokens, applied to another table's char-def.
                        char::from_u32(PUA_BASE + acd.0)
                            .expect("char table too large for the PUA token scheme")
                    })
                    .collect();
                chars.sort_unstable();
                chars.dedup();
                chars
            }
        }
    }

    /// Render a table-qualified cross-table `Segments` atom using the same feature-unification
    /// semantics as `pg_rules::bridge`. Raw ids are never reinterpreted in the owning table.
    pub(crate) fn render_foreign_constraint_tokens(
        &self,
        source_table: TableId,
        cd: CharDefId,
    ) -> Vec<char> {
        let (_, aliases) = self.aliasing.expect(
            "foreign Segments rendering requires a grammar-aware, table-qualified alphabet",
        );
        let mut chars: Vec<char> = aliases
            .feature_constraint_aliases_for(source_table, cd)
            .iter()
            .map(|(_, candidate)| {
                char::from_u32(PUA_BASE + candidate.0)
                    .expect("char table too large for the PUA token scheme")
            })
            .collect();
        chars.sort_unstable();
        chars.dedup();
        chars
    }
    /// Encode a `pg_shape::Shape`'s interior nodes (module doc's "already-segmented" shortcut —
    /// root/affix authored text is segmented once at grammar load; this just replays that Shape,
    /// never re-parsing the text) into one token string, Segment and Boundary nodes both kept (a
    /// rule's own context can reference either kind — Indonesian's boundary `char30` is itself
    /// just another char-def with its own token here, module doc).
    ///
    /// **Never aliases** (design item 4) — a concrete allomorph's own underlying spelling is not a
    /// rule pattern; this always calls `Self::token` directly, regardless of `Self::aliasing`.
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
    ///
    /// **Never aliases** (design item 4: "`encode_shape`/`encode_query` must NOT alias... aliasing
    /// there would make a query word ambiguous") — always `Self::token`, regardless of
    /// `Self::aliasing`.
    pub fn encode_query(&self, word: &str) -> Option<String> {
        let shape = pg_grammar::segment::segment_phonemes_only(self.table, word).ok()?;
        Some(
            shape
                .interior()
                .map(|(_, _, cd, _)| self.token(CharDefId(cd)))
                .collect(),
        )
    }

    /// True when every interior node of `shape` names a concrete `CharDefId` — false for an
    /// abstract, natural-class-derived node (`pg_shape::NO_CHAR_DEF`), which stands for a SET of
    /// segments and has no single codepoint of its own to hand `Self::token`.
    pub fn shape_is_tokenizable(shape: &pg_shape::Shape) -> bool {
        shape
            .interior()
            .all(|(_, _, cd, _)| cd != pg_shape::NO_CHAR_DEF)
    }

    pub fn table(&self) -> &'t CharDefTable {
        self.table
    }
}

/// True when some root allomorph in `g`'s lexicon carries a shape
/// [`SegAlphabet::shape_is_tokenizable`] declines AND `crate::emit::pattern_root_token_route`
/// also declines it — the pattern-language `[ClassName]` construct
/// (`machine:edge-cases/loader-pattern-shapes`'s `b[Vowel]t`) segments to an abstract node even
/// when it is not `RootAllomorphDef::is_pattern` (that flag only fires for an optional/iterative
/// node, never a mandatory class reference), so `crate::uflexc`/`crate::emit`'s own `is_pattern`
/// guard alone lets one through into `SegAlphabet::encode_shape` -- but `collect_roots` may still
/// route it through the token-space pattern route, so this calls that SAME decision rather than
/// re-deriving a wider one. Names `TemplatedUnderlyingTokens`'s own refusal condition
/// specifically — too weak a claim for `PlanComposed` (below), which tolerates a partial
/// lexicon; pinned by `the_published_untokenizable_root_shape_fact_never_over_claims_a_refusal`.
pub fn grammar_has_untokenizable_root_shape(g: &Grammar) -> bool {
    let alphabet = SegAlphabet::new(crate::emit::surface_table(g));
    g.strata.iter().any(|stratum| {
        let stratum_table = &g.char_tables[stratum.table.0 as usize];
        stratum.entries.iter().any(|&entry_id| {
            g.entries[entry_id.0 as usize]
                .allomorphs
                .iter()
                .any(|allo| {
                    !SegAlphabet::shape_is_tokenizable(&allo.shape.shape)
                        && crate::emit::pattern_root_token_route(
                            &alphabet,
                            stratum_table,
                            &allo.shape.shape,
                            allo.environments.is_empty(),
                        )
                        .is_none()
                })
        })
    })
}

/// True when EVERY root allomorph in `g`'s lexicon is `RootAllomorphDef::is_pattern` or a shape
/// [`SegAlphabet::shape_is_tokenizable`] declines — the condition under which
/// `crate::uflexc::emit_underlying_filtered` skips every root line, leaving `PlanComposed` with no
/// candidate to certify. Vacuously true for a lexicon with no allomorph at all; pinned by
/// `the_published_no_tokenizable_root_fact_never_over_claims_a_refusal`.
pub fn grammar_has_no_tokenizable_root(g: &Grammar) -> bool {
    g.entries.iter().all(|entry| {
        entry
            .allomorphs
            .iter()
            .all(|allo| allo.is_pattern || !SegAlphabet::shape_is_tokenizable(&allo.shape.shape))
    })
}

// Re-exported from `crate::lower` (canonical home) at the same paths so existing callers need no change.
pub(crate) use crate::lower::{pattern_slots, render_slots, resolve_alpha_tuples, Slot};
pub use crate::lower::{AlphaAssignment, TupleReport};

#[cfg(test)]
mod representation_alias_map_tests;

/// Resolves `rule`'s OWNING `CharDefTable` via its owning stratum's `StratumDef::table`.
/// Every compiled rule carries its owning character-table identity explicitly; table zero is
/// never an implicit default.
///
/// `rule` is looked up in `g.prules` by `xml_id` (document-unique, per the DTD's own `xs:ID`
/// discipline for every element's `id=` attribute — `pg_grammar::load`'s own convention) rather
/// than by pointer identity: `compile_rewrite_rule_subset` receives `rule: &RewriteRuleDef`
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
/// silent guess. `compile_rewrite_rule_subset` treats `None` exactly like an unsupported pattern
/// construct (`None`, reported `skipped` by its own caller); `capability.rs`'s
/// `lower_subrule_span` reports it as a span that will not lower (any approximation rounds toward
/// `Refuse`).
pub(crate) fn owning_table<'g>(g: &'g Grammar, rule: &RewriteRuleDef) -> Option<&'g CharDefTable> {
    let idx = g
        .prules
        .iter()
        .position(|pr| matches!(pr, PhonRuleDef::Rewrite(r) if r.xml_id == rule.xml_id))?;
    owning_table_for_prule_position(g, idx)
}

/// `owning_table`'s sibling returning the resolved `TableId` itself rather than the
/// `&CharDefTable` — needed by `compile_rewrite_rule_subset` to build a [`SegAlphabet::
/// with_table_id`] alphabet that can name itself for cross-table representation aliasing. Shares
/// `owning_table_id_for_prule_position` with `owning_table_for_prule_position` (both derive
/// from the exact SAME stratum lookup, never two independently-derived resolutions that could
/// silently disagree).
pub(crate) fn owning_table_id(g: &Grammar, rule: &RewriteRuleDef) -> Option<TableId> {
    let idx = g
        .prules
        .iter()
        .position(|pr| matches!(pr, PhonRuleDef::Rewrite(r) if r.xml_id == rule.xml_id))?;
    owning_table_id_for_prule_position(g, idx)
}

/// `owning_table`'s sibling for a `MetathesisRuleDef`: identical reasoning, just matched
/// against the `PhonRuleDef::Metathesis` variant instead of `PhonRuleDef::Rewrite` — a
/// `MetathesisRuleDef` lives in the SAME `g.prules`
/// vec and is wired to a stratum's own `prules: Vec<PRuleId>` list exactly the same way, so the
/// "find this rule's own index, then find which stratum's own cascade contains it" algorithm is
/// identical; only the variant match differs. Shares `owning_table_for_prule_position` with
/// `owning_table` rather than re-deriving the stratum lookup a second time.
pub(crate) fn owning_table_for_metathesis<'g>(
    g: &'g Grammar,
    rule: &MetathesisRuleDef,
) -> Option<&'g CharDefTable> {
    let idx = g
        .prules
        .iter()
        .position(|pr| matches!(pr, PhonRuleDef::Metathesis(r) if r.xml_id == rule.xml_id))?;
    owning_table_for_prule_position(g, idx)
}

/// `owning_table_for_metathesis`'s sibling returning the resolved `TableId` itself, needed by
/// `compile_metathesis_rule` to build a `SegAlphabet::with_table_id`-shaped alias context for
/// cross-table representation aliasing. Mirrors
/// `owning_table_id`'s own relationship to `owning_table`: shares
/// `owning_table_id_for_prule_position` with `owning_table_for_metathesis` (both derive from the
/// exact same stratum lookup, never two independently-derived resolutions that could silently
/// disagree).
pub(crate) fn owning_table_id_for_metathesis(
    g: &Grammar,
    rule: &MetathesisRuleDef,
) -> Option<TableId> {
    let idx = g
        .prules
        .iter()
        .position(|pr| matches!(pr, PhonRuleDef::Metathesis(r) if r.xml_id == rule.xml_id))?;
    owning_table_id_for_prule_position(g, idx)
}

/// Shared tail of `owning_table`/`owning_table_for_metathesis`: finds the stratum owning this rule position's table, or `None` if wired into none.
fn owning_table_for_prule_position(g: &Grammar, idx: usize) -> Option<&CharDefTable> {
    let table_id = owning_table_id_for_prule_position(g, idx)?;
    Some(&g.char_tables[table_id.0 as usize])
}

/// `owning_table_for_prule_position`'s own `TableId` half, factored out so `owning_table_id` shares this lookup rather than re-deriving it.
fn owning_table_id_for_prule_position(g: &Grammar, idx: usize) -> Option<TableId> {
    let target = PRuleId(idx as u32);
    let stratum = g.strata.iter().find(|s| s.prules.contains(&target))?;
    Some(stratum.table)
}

/// `slots` in reverse document order; recurses into a `Slot::Repeat`'s own `children`, or a heterogeneous quantifier group compiles a wrong RTL branch net.
/// See `docs/research/pg-foma-replace-design-notes.md`, "`reversed_slots`: why it must recurse into `Slot::Repeat` children".
fn reversed_slots(slots: &[Slot]) -> Vec<Slot> {
    slots
        .iter()
        .rev()
        .map(|s| match s {
            Slot::Repeat { min, max, children } => Slot::Repeat {
                min: *min,
                max: *max,
                children: reversed_slots(children),
            },
            other => other.clone(),
        })
        .collect()
}

/// Renders one complete branch's xre source text. Empty `lhs`/`rhs` render as `"[..]"`/`"0"`: foma's xre grammar requires a non-blank operand.
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

/// Compiles one branch into a foma `Fsm`: `Dir::LeftToRight` renders the slots as given; `Dir::RightToLeft` unions that with `fsm_reverse` of the mirror rule's compile (LHS/RHS reversed, environments swapped-and-reversed).
/// Worked example and why the union introduces no spurious identity path: `docs/research/pg-foma-replace-design-notes.md`, "`compile_rtl_branch_net`: worked example pinning the union's necessity".
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
    rule_xml_id: &str,
) -> Fsm {
    let plain_regex =
        render_branch_regex(alphabet, lhs_slots, rhs_slots, left_slots, right_slots, asg);
    let plain_net = fsm_parse_regex(opts, &plain_regex, None, None).unwrap_or_else(|| {
        panic!("foma rejected compiled regex for rule {rule_xml_id}: {plain_regex:?}")
    });
    match dir {
        Dir::LeftToRight => plain_net,
        Dir::RightToLeft => {
            let mirror_lhs = reversed_slots(lhs_slots);
            let mirror_rhs = reversed_slots(rhs_slots);
            // Swap: the mirror rule's own left environment is the reversed original right environment, and vice versa.
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
            let mirror_net =
                fsm_parse_regex(opts, &mirror_regex, None, None).unwrap_or_else(|| {
                    panic!(
                        "foma rejected compiled mirror-rule regex for rule {rule_xml_id}: \
                     {mirror_regex:?}"
                    )
                });
            let reversed_net = fsm_reverse(mirror_net);
            fsm_union(opts, plain_net, reversed_net)
        }
    }
}

// One subrule compiles to one or more xre replace-rule instances, composed (not unioned) via fsm_compose: alpha tuples are mutually exclusive by construction, so sequential composition is correct.
// Two rejected alternatives: docs/research/pg-foma-replace-design-notes.md, "Per-subrule composition: two rejected constructions".

/// Compile one `RewriteRuleDef` (all its subrules, all their alpha tuples) into ONE foma `Fsm`
/// (union of every subrule × tuple instance), and report the alpha-tuple expansion for every
/// alpha-bearing subrule (empty if the rule uses no alpha variables). Returns `None` if any
/// subrule's pattern needs an unsupported construct (module doc's scope list) — the CALLER
/// decides whether to skip that rule (reported uncovered) or treat the whole compile as failed;
/// this prototype's driver skips and reports.
///
/// Thin wrapper over `compile_rewrite_rule_subset` that includes every subrule (the pre-gating
/// behavior, unchanged for every existing caller).
pub fn compile_rewrite_rule(
    opts: &FomaOptions,
    g: &Grammar,
    rule: &RewriteRuleDef,
) -> Option<(Fsm, Vec<TupleReport>)> {
    compile_rewrite_rule_subset(opts, g, rule, &|_| true)
}

/// Whether `compile_rewrite_rule_subset` could produce a net for `rule` at all when every subrule
/// is allowed, decided from the same shape/pattern-lowering checks that function runs before it
/// ever builds an `Fsm` -- so a capability-envelope caller reads one computation with the real
/// compiler rather than re-deriving an equivalent-looking check (this repo's own CLAUDE.md names
/// three past attempts that failed exactly that way).
pub fn rewrite_rule_is_lowerable(g: &Grammar, rule: &RewriteRuleDef) -> bool {
    if !is_fully_supported_shape(g, rule) {
        return false;
    }
    let Some(table) = owning_table(g, rule) else {
        return false;
    };
    let scope = crate::lower::PatternLowerScope::RewriteRuleCompile;
    // A zero-subrule rule builds no net either (the real loop below never runs), so it is not lowerable.
    !rule.subrules.is_empty()
        && rule.subrules.iter().all(|subrule| {
            let mut next_occurrence = 0usize;
            pattern_slots(g, table, &rule.lhs, &mut next_occurrence, scope).is_some()
                && pattern_slots(g, table, &subrule.rhs, &mut next_occurrence, scope).is_some()
                && match &subrule.left_env {
                    Some(p) => pattern_slots(g, table, p, &mut next_occurrence, scope).is_some(),
                    None => true,
                }
                && match &subrule.right_env {
                    Some(p) => pattern_slots(g, table, p, &mut next_occurrence, scope).is_some(),
                    None => true,
                }
        })
}

/// Identical to `compile_rewrite_rule`, but SKIPS any subrule for which `allowed(subrule_index)`
/// is `false` (document order, `0`-based into `rule.subrules`) — the MPR/POS gating mechanism
/// (`crate::gate`): a subrule declaring `requiredPartsOfSpeech`/`requiredMPRFeatures`/
/// `excludedMPRFeatures` must not compile into a network branch that a NON-eligible lexical entry's
/// group can reach (module doc "static partition" design in `crate::gate`). Returns `None` if
/// EVERY subrule is either filtered out or hits an unsupported construct — the caller (per-group
/// rule cascade builder) treats that identically to "this rule doesn't fire in this group": the
/// whole rule is simply absent from the group's composed cascade (identity), not an error. This is
/// the same `None` the pre-gating code already used for "unsupported construct", so no NEW branch
/// is introduced at any call site — see `compile_and_compose_rules_gated`'s doc for the one
/// known imprecision this shares with the ungated path (a rule with one unsupported subrule and one
/// supported-but-gated subrule reports the WHOLE rule uncovered for every group, matching
/// `compile_rewrite_rule`'s own pre-existing all-or-nothing `?` short-circuit — not a regression).
///
/// **Mode/dir detection:**
/// `rule.mode`/`rule.dir` are checked FIRST, via `is_fully_supported_shape` -- a rule outside
/// that shape returns `None` immediately, exactly the same "uncovered, caller reports it
/// `skipped`" contract `pattern_slots` already uses for an unsupported PATTERN construct (a
/// malformed `Quantifier` or a disagree-polarity alpha var -- cross-table and same-table
/// `Segments` plus any `Anchor` no longer disqualify a rewrite rule's own pattern at all, per this
/// function's own `PatternLowerScope::RewriteRuleCompile` call below). Before this check existed,
/// an unsupported mode/dir was silently
/// compiled via plain foma `->` as if it were Iterative/LeftToRight -- a WRONG network with no
/// signal ("silent mis-map"). `Dir::RightToLeft` used to be gated out here too
/// (`None`, honestly skipped) until it gained
/// real semantics (`compile_rtl_branch_net`, module doc) -- both `Iterative` directions now
/// compile unconditionally. `RewriteMode::Simultaneous` used to be gated out here UNCONDITIONALLY
/// too, until `is_fully_supported_shape` gained a
/// per-rule admission check for it (that function's own doc) -- a `Simultaneous` rule whose
/// subrules the `simultaneous.subrule-overlap` predicate proves pairwise non-overlapping now
/// compiles via this SAME sequential-compose loop, unmodified; one the predicate cannot clear
/// stays gated here exactly as before. Every reference-grammar rule (Indonesian/Amharic/Sena) is
/// already `Iterative`/`LeftToRight`, so none of
/// these three changes alters any existing grammar's compiled output -- verified by
/// `tests/p6_gate_parity.rs`'s byte-exact Amharic state/arc-count regression guard and
/// `tests/f3_parity.rs`'s multiset parity gates staying green.
pub fn compile_rewrite_rule_subset(
    opts: &FomaOptions,
    g: &Grammar,
    rule: &RewriteRuleDef,
    allowed: &dyn Fn(usize) -> bool,
) -> Option<(Fsm, Vec<TupleReport>)> {
    if !is_fully_supported_shape(g, rule) {
        return None;
    }
    // Resolved once per rule, never an implicit table-zero default; `None` is treated like an unsupported construct, reported `skipped` by callers.
    let table = owning_table(g, rule)?;
    // `owning_table_id` shares `owning_table`'s own stratum lookup, so it is guaranteed `Some` here too.
    let table_id = owning_table_id(g, rule)
        .expect("owning_table_id shares owning_table's own lookup, which just resolved Some");
    // Built once per rule rather than threaded from the outer cascade functions.
    let alias_map = RepresentationAliasMap::build(g);
    let render_alphabet = SegAlphabet::with_table_id(table, table_id, &alias_map);
    let mut net: Option<Fsm> = None;
    let mut reports: Vec<TupleReport> = Vec::new();

    for (subrule_index, subrule) in rule.subrules.iter().enumerate() {
        if !allowed(subrule_index) {
            continue;
        }
        // Alpha slots are numbered fresh per subrule (HC's variable scoping is per-subrule), so `lhs_slots` is recomputed here, not hoisted above the loop.
        let mut next_occurrence = 0usize;
        // `crate::capability::rtl_reversal_construction_attempted` must pass this same scope value, or the capability predicate and this compiler could silently diverge on which rules are admitted.
        let scope = crate::lower::PatternLowerScope::RewriteRuleCompile;
        let lhs_slots = pattern_slots(g, table, &rule.lhs, &mut next_occurrence, scope)?;
        let rhs_slots = pattern_slots(g, table, &subrule.rhs, &mut next_occurrence, scope)?;
        let left_slots = match &subrule.left_env {
            Some(p) => pattern_slots(g, table, p, &mut next_occurrence, scope)?,
            None => Vec::new(),
        };
        let right_slots = match &subrule.right_env {
            Some(p) => pattern_slots(g, table, p, &mut next_occurrence, scope)?,
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
        reports.push(report);

        for asg in &assignments {
            let branch_net = compile_rtl_branch_net(
                opts,
                &render_alphabet,
                rule.dir,
                &lhs_slots,
                &rhs_slots,
                &left_slots,
                &right_slots,
                asg,
                &rule.xml_id,
            );
            net = Some(match net {
                None => branch_net,
                // Sequential composition, not union -- see the per-subrule composition note above this loop.
                Some(prev) => fsm_compose(opts, prev, branch_net),
            });
        }
    }

    net.map(|n| (n, reports))
}

/// Compile every `Rewrite`-kind `PhonRuleDef` in `stratum_prules` order into individual foma
/// nets and left-fold-compose them via `foma::constructions::fsm_compose` (stratum/document order = feeding order —
/// prule4's assimilated output is prule5's own deletion-context input, verified by hand against
/// `menulis`/`memukul`). `Metathesis`-kind rules and any
/// `Rewrite` rule this module can't render are skipped, their `xml_id`s returned in `skipped` so
/// the caller can report them (never silently dropped).
///
/// Returns `None` if there are zero compilable rules at all (the composition would be a no-op —
/// callers should compose with an identity net instead of calling this).
///
/// Deliberately NOT given a final minimize call (unlike `crate::gate::
/// compile_gated_grammar`): `tests/p6_gate_parity.rs`'s
/// `amharic_gated_subrules_and_tuple_counts_unregressed` hard-asserts this function's return value
/// is BYTE IDENTICAL to a fixed state/arc count (82 states / 1,110,358 arcs) with no minimize
/// applied by this function itself -- adding one here would change those counts (composing minimal
/// nets is not itself guaranteed minimal) and break that regression guard. Callers that want a
/// minimal composed rule net should call
/// `foma::minimize::fsm_minimize` themselves (every example driver already does, on the FULL
/// `lexc .o. rules .o. cleanup` composition, not on this
/// function's return value alone).
pub fn compile_and_compose_rules(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    skipped: &mut Vec<String>,
    tuple_reports: &mut Vec<(String, Vec<TupleReport>)>,
) -> Option<Fsm> {
    compile_and_compose_rules_internal(
        opts,
        g,
        alphabet,
        prules_in_order,
        skipped,
        tuple_reports,
        false,
    )
}

/// Compile the same ordered cascade for a propose-then-confirm caller, but preserve an identity
/// alternative at EVERY rewrite, not only RTL ones. RTL's own construction is a known-approximate
/// safe superset (the original reason this existed), but a `LeftToRight`/`Simultaneous` rule can
/// lose recall the same way for a reason RTL doesn't have: `crate::lower::class_members`'s
/// `Feature`-kind matching has no access to `UseDefaults`/`defaultSymbol` (that is a
/// `pg_rules::rewrite`-confirm-only concept -- `pg_grammar::chardef`'s own `feature_lanes` default
/// an unspecified lane to "matches anything", per `build_feature_lanes`'s doc), and a caller with
/// gated subrules (`crate::gate::find_gated_subrules`) applies that gating per lexical-entry group
/// before calling this file's OWN gated compiler -- this ungated cascade has no such group split at
/// all, so every subrule here compiles as if it always licenses, regardless of the POS/MPR state
/// the confirm engine would actually require. Both gaps have the identical shape RTL's own doc
/// names: an obligatory FST rewrite whose LHS match is a superset of the truth removes the
/// UNMUTATED candidate outright rather than merely adding a false one, which is a propose-stage
/// recall loss a confirm-side check cannot recover from. Making every stage optional here defers
/// the real decision to confirm exactly like RTL already does, and costs nothing more than RTL's
/// own union already costs per rule.
pub fn compile_and_compose_rules_recall_safe(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    skipped: &mut Vec<String>,
    tuple_reports: &mut Vec<(String, Vec<TupleReport>)>,
) -> Option<Fsm> {
    compile_and_compose_rules_internal(
        opts,
        g,
        alphabet,
        prules_in_order,
        skipped,
        tuple_reports,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn compile_and_compose_rules_internal(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    prules_in_order: &[&PhonRuleDef],
    skipped: &mut Vec<String>,
    tuple_reports: &mut Vec<(String, Vec<TupleReport>)>,
    optional_every_rule: bool,
) -> Option<Fsm> {
    let mut composed: Option<Fsm> = None;
    for pr in prules_in_order {
        let rule = match pr {
            PhonRuleDef::Rewrite(rule) => rule,
            PhonRuleDef::Metathesis(m) => {
                // A shape left honestly unsupported falls through to the same `skipped` report every unsupported construct uses, never a silent wrong compile.
                match compile_metathesis_rule(opts, g, alphabet, m) {
                    Some(net) => {
                        composed = Some(match composed {
                            None => net,
                            Some(prev) => fsm_compose(opts, prev, net),
                        });
                    }
                    None => skipped.push(format!("{} (metathesis, unhandled)", m.xml_id)),
                }
                continue;
            }
        };
        // `is_fully_supported_shape` detects an unsupported RightToLeft/Simultaneous shape and reports it `skipped`, never a silent mis-map.
        match compile_rewrite_rule_subset(opts, g, rule, &|_| true) {
            Some((net, reports)) => {
                tuple_reports.push((rule.xml_id.clone(), reports));
                let net = if optional_every_rule {
                    fsm_union(opts, net, fsm_universal())
                } else {
                    net
                };
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

/// Identical to `compile_and_compose_rules`, but for ONE GATING GROUP (`crate::gate`): for every
/// `Rewrite`-kind rule at position `rule_pos` in `prules_in_order`, `subrule_ok(rule_pos, sub_idx)`
/// decides whether that specific subrule is included for THIS group (module doc: a group is a set
/// of lexical entries that agree on every gated subrule's applicability, so ungated subrules always
/// pass `subrule_ok` unconditionally — only `crate::gate`'s own gated-subrule list ever returns
/// `false`). A rule whose every subrule is filtered out for this group is skipped exactly like an
/// unsupported-construct rule (absent from the group's cascade, i.e. identity for this group) —
/// see `compile_rewrite_rule_subset`'s doc.
///
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
        let rule = match pr {
            PhonRuleDef::Rewrite(rule) => rule,
            PhonRuleDef::Metathesis(m) => {
                // A `MetathesisRuleDef` carries no subrules, so gating does not apply -- every group compiles the same metathesis relation.
                match compile_metathesis_rule(opts, g, alphabet, m) {
                    Some(net) => {
                        composed = Some(match composed {
                            None => net,
                            Some(prev) => fsm_compose(opts, prev, net),
                        });
                    }
                    None => skipped.push(format!("{} (metathesis, unhandled)", m.xml_id)),
                }
                continue;
            }
        };
        // Mode/dir detection lives in `compile_rewrite_rule_subset` (`is_fully_supported_shape`), so an unsupported shape is reported `skipped`, never silently mis-compiled.
        let allowed = |sub_idx: usize| subrule_ok(rule_pos, sub_idx);
        match compile_rewrite_rule_subset(opts, g, rule, &allowed) {
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

/// `true` iff `rule.mode` (and, for `Simultaneous`, `rule`'s own subrule shape against `g`) is a
/// shape this file's compile functions claim fidelity for. `RewriteMode::Iterative` compiles under
/// EITHER `Dir` (`Dir::LeftToRight` via the plain construction; `Dir::RightToLeft` via
/// `compile_rtl_branch_net`'s reversal-plus-safety-net-union construction), unconditionally
/// in-shape regardless of subrule content.
///
/// `RewriteMode::Simultaneous`: NOT
/// wholesale in/out of shape the way `Iterative`/`RightToLeft` are -- admitted *unless* two of
/// `rule`'s own subrules' environments can match at the same input position
/// (`crate::capability::simultaneous_rule_admitted_for_compile`, the SAME `simultaneous.subrule-
/// overlap` proof the capability GATE's own `SimultaneousSubruleOverlapPredicate` uses — one
/// shared algorithm, two call sites, so the gate and this compiler can never disagree). When
/// admitted, this file's EXISTING plain/iterative sequential-compose machinery
/// (`compile_rewrite_rule_subset`'s own per-subrule `fsm_compose` fold, unchanged code) is used
/// as-is: the admitted case's own defining property is that simultaneous application == sequential
/// application at every position (no two subrules can ever contest the same focus), so reusing
/// that machinery is not an approximation, it is the correct construction. A rule the predicate
/// cannot prove non-overlapping for (or with a self-opaquing subrule, or an unsupported pattern
/// node in a lowered span) stays OUTSIDE this shape -- `compile_rewrite_rule_subset` returns
/// `None` for it exactly like any other unsupported construct, honest-unsupported, never a
/// wrong compile.
pub fn is_fully_supported_shape(g: &Grammar, rule: &RewriteRuleDef) -> bool {
    match rule.mode {
        RewriteMode::Iterative => true,
        RewriteMode::Simultaneous => {
            crate::capability::simultaneous_rule_admitted_for_compile(g, rule).is_ok()
        }
    }
}

/// Convenience re-export so the driver doesn't need a second `use` line for the one subrule field
/// this module reads directly (`mode`/`dir` are read via `is_fully_supported_shape` instead).
pub type Subrule = RewriteSubruleDef;

// Metathesis: the dedicated swap relation. Disposition is ConfirmOnly, never Admit, both directions.
// See docs/research/pg-foma-replace-design-notes.md, "Metathesis: the dedicated swap relation".

/// Every `CharDefId` a slot may resolve to, with cross-table representation aliasing applied member-by-member; `None` for `Slot::Alpha`/`Slot::Repeat`/`Slot::Anchor`.
/// See `docs/research/pg-foma-replace-design-notes.md`, "Metathesis: the dedicated swap relation".
fn slot_candidates(
    slot: &Slot,
    table: &CharDefTable,
    table_id: TableId,
    aliases: &RepresentationAliasMap,
) -> Option<Vec<CharDefId>> {
    let expand = |members: &[CharDefId]| -> Vec<CharDefId> {
        let mut out: Vec<CharDefId> = Vec::with_capacity(members.len());
        for &cd in members {
            for (_tid, acd) in aliases.aliases_for(table, table_id, cd) {
                if !out.contains(&acd) {
                    out.push(acd);
                }
            }
        }
        out
    };
    match slot {
        Slot::Fixed(cd) => Some(expand(std::slice::from_ref(cd))),
        Slot::Union(members) => Some(expand(members)),
        Slot::ForeignFixed { .. } | Slot::Alpha { .. } | Slot::Repeat { .. } | Slot::Anchor => None,
    }
}

/// The per-branch literal cross-product swap construction, shared by the plain and mirror (RTL) orientations; `left_idx`/`right_idx` index into `slots` as given.
#[allow(clippy::too_many_arguments)]
fn compile_metathesis_swap_net(
    opts: &FomaOptions,
    alphabet: &SegAlphabet,
    slots: &[Slot],
    left_idx: usize,
    right_idx: usize,
    rule_xml_id: &str,
    table: &CharDefTable,
    table_id: TableId,
    aliases: &RepresentationAliasMap,
) -> Option<Fsm> {
    let leading_anchor = matches!(slots.first(), Some(Slot::Anchor));
    let trailing_anchor = matches!(slots.last(), Some(Slot::Anchor));
    let start = usize::from(leading_anchor);
    let end = slots.len().saturating_sub(usize::from(trailing_anchor));
    if slots[start..end]
        .iter()
        .any(|slot| matches!(slot, Slot::Anchor))
        || left_idx < start
        || right_idx < start
        || left_idx >= end
        || right_idx >= end
    {
        return None;
    }
    let effective_slots = &slots[start..end];
    let adjusted_left = left_idx - start;
    let adjusted_right = right_idx - start;
    let (lo, hi) = (
        adjusted_left.min(adjusted_right),
        adjusted_left.max(adjusted_right),
    );

    let mut candidates: Vec<Vec<CharDefId>> = Vec::with_capacity(effective_slots.len());
    for slot in effective_slots {
        match slot_candidates(slot, table, table_id, aliases) {
            Some(members) if !members.is_empty() => candidates.push(members),
            _ => return None, // Slot::Alpha/Slot::Repeat, or a vacuous empty class.
        }
    }

    // No joint-agreement filter: metathesis has no shared-VarId constraint between positions.
    let mut assignments: Vec<Vec<CharDefId>> = vec![Vec::with_capacity(effective_slots.len())];
    for members in &candidates {
        let mut next = Vec::with_capacity(assignments.len() * members.len());
        for asg in &assignments {
            for &cd in members {
                let mut a = asg.clone();
                a.push(cd);
                next.push(a);
            }
        }
        assignments = next;
    }

    let mut net: Option<Fsm> = None;
    for asg in &assignments {
        let mut rhs_vals = asg.clone();
        rhs_vals.swap(lo, hi);
        let lhs_text = asg
            .iter()
            .map(|cd| alphabet.token(*cd).to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let rhs_text = rhs_vals
            .iter()
            .map(|cd| alphabet.token(*cd).to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let regex = format!("{lhs_text} -> {rhs_text}");
        let branch_net = fsm_parse_regex(opts, &regex, None, None).unwrap_or_else(|| {
            panic!("foma rejected compiled metathesis regex for rule {rule_xml_id}: {regex:?}")
        });
        net = Some(match net {
            None => branch_net,
            Some(prev) => fsm_union(opts, prev, branch_net),
        });
    }
    net
}

/// The `Dir::RightToLeft` mirror's own switch indices: `(n - 1 - left_idx, n - 1 - right_idx)`. Factored out so `metathesis_mirror_switch_index_remap_tests` can pin the arithmetic without building a whole `Fsm`.
/// Derivation: `docs/research/pg-foma-replace-design-notes.md`, "Metathesis: the dedicated swap relation".
fn metathesis_mirror_switch_indices(n: usize, left_idx: usize, right_idx: usize) -> (usize, usize) {
    (n - 1 - left_idx, n - 1 - right_idx)
}

#[cfg(test)]
mod metathesis_mirror_switch_index_remap_tests;

/// Compiles one `MetathesisRuleDef` into the dedicated swap relation (module doc above), or
/// `None` for a shape this change leaves honestly unsupported — the SAME "uncovered, caller
/// reports it `skipped`" contract `compile_rewrite_rule_subset` already uses for an unsupported
/// `RewriteRuleDef` pattern construct.
///
/// `Dir::LeftToRight` compiles via `compile_metathesis_swap_net` alone (byte-identical to what
/// this function has always done for a grammar with no cross-table shared representation — no
/// behavior change for any such `Dir::LeftToRight` rule).
/// `Dir::RightToLeft` (module doc's
/// own "`Dir::RightToLeft`" section above for the full construction/remap derivation) additionally
/// mirrors the pattern via `reversed_slots`, remaps the two switch indices, compiles the mirror's
/// own swap net, `fsm_reverse`s it, and unions that with the plain net — the SAME four moves
/// `compile_rtl_branch_net` already makes for RTL rewrite rules.
///
/// Builds its own `RepresentationAliasMap`/owning `TableId` and threads them into every
/// `compile_metathesis_swap_net` call (module doc's "Cross-table representation aliasing"
/// section) — mirroring `compile_rewrite_rule_subset`'s own per-rule alias-map construction, so a
/// `MetathesisRule` in a grammar whose tables share a normalized representation gets the SAME
/// render-time recall fix a `RewriteRuleDef` already does.
pub(crate) fn compile_metathesis_rule(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    rule: &MetathesisRuleDef,
) -> Option<Fsm> {
    let table = owning_table_for_metathesis(g, rule)?;
    // Shares `owning_table_for_metathesis`'s own stratum lookup, so this is guaranteed `Some` here too.
    let table_id = owning_table_id_for_metathesis(g, rule)
        .expect("owning_table_id_for_metathesis shares owning_table_for_metathesis's own lookup, which just resolved Some");
    let alias_map = RepresentationAliasMap::build(g);
    let mut next_occurrence = 0usize;
    // Must stay in lockstep with `capability::metathesis_swap_construction_attempted`'s own scope, or the two could admit different rule sets.
    let scope = crate::lower::PatternLowerScope::RewriteRuleCompile;
    let slots = pattern_slots(g, table, &rule.pattern, &mut next_occurrence, scope)?;
    let left_idx = rule.left_switch as usize;
    let right_idx = rule.right_switch as usize;
    if left_idx == right_idx || left_idx >= slots.len() || right_idx >= slots.len() {
        // Defensive: both are always in bounds by construction; honest-unsupported rather than a panic if that ever changes.
        return None;
    }

    let plain_net = compile_metathesis_swap_net(
        opts,
        alphabet,
        &slots,
        left_idx,
        right_idx,
        &rule.xml_id,
        table,
        table_id,
        &alias_map,
    )?;

    match rule.dir {
        Dir::LeftToRight => Some(plain_net),
        Dir::RightToLeft => {
            // `slots` never contains a `Slot::Repeat`/`Slot::Alpha` here, so every slot is atomic and `reversed_slots` is a pure index reversal.
            let mirror_slots = reversed_slots(&slots);
            let (mirror_left_idx, mirror_right_idx) =
                metathesis_mirror_switch_indices(slots.len(), left_idx, right_idx);
            let Some(mirror_net) = compile_metathesis_swap_net(
                opts,
                alphabet,
                &mirror_slots,
                mirror_left_idx,
                mirror_right_idx,
                &rule.xml_id,
                table,
                table_id,
                &alias_map,
            ) else {
                // Kept as an honest `None` rather than `unreachable!` in case this equivalence is ever violated.
                return None;
            };
            let reversed_net = fsm_reverse(mirror_net);
            let unioned = fsm_union(opts, plain_net, reversed_net);
            Some(unioned)
        }
    }
}

/// A positive witness for `owning_table`: two tables with different segment counts (2 vs 3), so an alpha-bound rule's `surviving` tuple count directly reveals which table it resolved against.
#[cfg(test)]
mod owning_table_tests;

#[cfg(test)]
mod rtl_repeat_children_reversal_tests;
