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
//! members, then keep only the combinations where every pair of same-`VarId` slots agrees (same
//! symbolic-feature value at that variable's lane — `AlphaVar::plus` polarity; `minus`/"disagree"
//! is unimplemented, see the doc on `AlphaOccurrence`). This bounds the count of segment tuples
//! satisfying the joint constraint (Amharic's 20-variable CV-merger: nc15=59 × nc16=6 ⇒ ≤354, never
//! v^20) — implemented once, generically over N variables and N slots-per-variable, so the same
//! code path that resolves Indonesian's single-variable prule4 is what would resolve Amharic's
//! rule without modification.
//!
//! ## What this module does NOT attempt
//! - `pg_grammar::model::PatternNode::Quantifier` (`OptionalSegmentSequence`) that is inverted (`min > max`, `max`
//!   concrete), pathologically large (a concrete `max` past `MAX_QUANTIFIER_BOUND`), or carries
//!   an alpha-bound occurrence anywhere in its own children — `pattern_slots` still returns
//!   `None`/bails for exactly these configurations (a rule whose pattern needs one is reported
//!   uncovered, not silently mis-rendered). A FINITELY bounded, alpha-free quantifier (`min`/`max`
//!   both concrete, `min <= max <= MAX_QUANTIFIER_BOUND`) compiles via `Slot::Repeat`,
//!   and a genuinely UNBOUNDED, alpha-free quantifier (`max ==
//!   None`, the DTD's `max="-1"` sentinel) now ALSO compiles, via that SAME `Slot::Repeat`
//!   (`max: Option<u32>`), rendered with foma's native `E*`/`E^>N` operator instead of `E^{min,max}`
//!   — see that variant's own doc for the construction, and "Bounded quantifiers" below for the
//!   compiled-vs-still-unsupported line and the confirm-engine finding that motivates it.
//! - `pg_grammar::model::AlphaVar::plus` == `false` ("disagree" polarity) — no reference-grammar rule needs it.
//! - `RewriteMode::Simultaneous` whose subrules the `simultaneous.subrule-overlap` predicate
//!   (`crate::capability`) cannot prove pairwise non-overlapping (self-opaquing, an unresolved
//!   overlap, or an unsupported pattern node in a lowered span) — see "`RewriteMode::Simultaneous`:
//!   compiling the ADMITTED case" below for the (now real) admitted case.
//! - MPR gating (`required_mpr`/`excluded_mpr` on a subrule) — flag-diacritic emission is
//!   out of scope, not attempted in this slice.
//!
//! ## `Dir::RightToLeft`: the reversal construction
//! `Dir::RightToLeft` used to be honestly skipped (the same `Ok(None)` treatment `Simultaneous`
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
//! `RewriteMode::Simultaneous` used to be honestly skipped UNCONDITIONALLY (`Ok(None)` for every
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
//! Now a FINITELY bounded, alpha-free quantifier — `max == Some(_)`, `min <= max <=
//! [MAX_QUANTIFIER_BOUND]`, no `Slot::Alpha` occurrence anywhere in its own (possibly nested)
//! children — compiles to a new `Slot::Repeat`, rendered as foma's OWN native bounded-repetition
//! xre operator, `A^{min,max}` (`nfst-xre = "0.1.0"`'s `RepeatNToK`, confirmed by reading that
//! vendored crate's own `src/lexer.rs`/`src/parser.rs`: `^{N,K}`/`^N,K` lexes to `CatenateNToK`, a
//! POSTFIX operator over whatever `[...]`-grouped term precedes it) over the quantifier's own
//! rendered children — never a hand-rolled state-machine construction, so this file inherits
//! foma's own `fsm_concat_m_n` construction (`foma = "0.4.0"`'s own `src/constructions/boolean.rs`:
//! `min` mandatory concatenated copies of the child net, then `max - min` further copies each
//! wrapped in `fsm_optionality` — i.e. **exactly** the "bounded concatenation/optionality"
//! construction this change's own proposal names, not an approximation of it) for free. Inverted
//! (`min > max`, `max` concrete, no sound finite construction), over-`MAX_QUANTIFIER_BOUND`
//! (`max` concrete), or alpha-nested quantifiers are UNCHANGED: still `None`, still honestly
//! reported uncovered by every existing caller.
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
//! rendering `min-1`, never `min`). `MAX_QUANTIFIER_BOUND` is never checked for this case
//! (`crate::lower`'s own doc on that constant): a Kleene star/plus's own compiled net size does not
//! depend on any repetition count at all, so "the bound is above the ceiling" is not even a
//! meaningful question to ask of `max: None` — this is a DIFFERENT native construction, not a
//! finite one that happens to be very large, and `max: None` is never coerced to a concrete number
//! anywhere in this path (a finite cutoff must never masquerade as unbounded semantics —
//! this is the SAME rule the original refusal existed to enforce, now honored by actually building
//! the unbounded construction instead of refusing every quantifier that might need it). Inverted/
//! over-`MAX_QUANTIFIER_BOUND`/alpha-nested quantifiers stay `None` exactly as before — those
//! checks are about a FINITE `max`'s own value and do not apply when there is no finite value to
//! check (`crate::lower::slots_from_nodes`'s own Quantifier arm skips them entirely for `max: None`).
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
//! shape [`ComposeBudget::tuple_cap`](crate::compose_budget::ComposeBudget::tuple_cap)'s own V3
//! check already guards on the alpha axis; the quantifier axis gets its OWN eager, cheaper-than-any-
//! `Fsm` characterization (`MAX_QUANTIFIER_BOUND`, checked in `pattern_slots` before any regex is even
//! rendered, let alone parsed — the same "check the search result before the expensive part"
//! principle [`ComposeBudget::tuple_cap`](crate::compose_budget::ComposeBudget::tuple_cap) already
//! uses for alpha tuples),
//! rather than a new `crate::compose_budget::ComposeBudget` dimension: `pattern_slots` is a pure
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
//! unconditionally, for EVERY caller alike — the RTL predicate's own witness listed them alongside
//! a malformed `Quantifier`/a disagree-polarity alpha var as the shapes `compile_rtl_branch_net`
//! excludes. Re-examining each one at the reversal construction's own level (`crate::lower::
//! PatternLowerScope`'s own doc has the full per-consumer boundary this section only summarizes):
//! - **`Segments` (same or different table).** Same-table literals lower to ordinary
//!   `crate::lower::Slot::Fixed` atoms. Cross-table literals lower to table-qualified
//!   `crate::lower::Slot::ForeignFixed` atoms and render as the union of owning-table tokens whose
//!   feature lanes unify with the foreign segment, matching the oracle without reinterpreting raw
//!   ids across tables. Both remain atomic under reversal.
//! - **`Anchor` (word-boundary condition).** Lowers to a new `crate::lower::Slot::Anchor`,
//!   rendered as foma's own `.#.` xre atom — IDENTICAL text regardless of [`pg_grammar::model::
//!   AnchorSide`] (`Slot::Anchor`'s own doc has the full argument for why the tag itself never
//!   needs inspecting: POSITION, not the tag, conveys word-initial vs. word-final). This is exactly
//!   why the mirror-and-reverse construction swaps an anchor to the CORRECT opposite edge with
//!   ZERO new code in `compile_rtl_branch_net`/`reversed_slots` themselves: an `Anchor(Right)`
//!   that is the LAST slot of the original `right_env` becomes, via the EXISTING `reversed_slots`
//!   (pure position reversal, no anchor-specific case) plus the EXISTING left/right swap, the FIRST
//!   slot of the mirror's own `left_env` — a leading `.#.` there means "start of the
//!   mirror/reversed representation", which `fsm_reverse` then correctly turns into "end of the
//!   real string" for the final network, by the SAME "reversing a network that operates on
//!   reversed strings gives back a network operating on normal strings" argument this file's own
//!   RTL section above already makes for ordinary content. Pinned empirically (not just argued):
//!   `tests/phase_c_right_to_left.rs`'s `rtl_anchor_reversal_swaps_the_correct_edge`.
//! - **A disagree-polarity alpha var** (`AlphaVar::plus == false`) stays refused, deliberately, on
//!   BOTH `Dir`s and EVERY caller — this is genuinely orthogonal to reversal (it is a gap in
//!   `resolve_alpha_tuples`' own joint-agreement filter, which only ever implements "agree" via
//!   bitwise overlap, never "disagree" via bitwise non-overlap/complement — the SAME gap for an
//!   ordinary `LeftToRight` rule), not something the mirror-and-reverse construction has any
//!   bearing on; building "disagree" semantics is a standalone `resolve_alpha_tuples` feature, out
//!   of this task's own scope (`crate::lower::UnsupportedPatternNode::AlphaDisagreePolarity`'s own
//!   doc; `crate::capability::RightToLeftRewriteFaithfulReversalPredicate`'s own tests pin this
//!   refusal with this specific named witness).
//! - **This widening is scope-gated** (`crate::lower::PatternLowerScope`), not a blanket change:
//!   `crate::lower::lower_span`'s own callers are unaffected, still passing
//!   `crate::lower::PatternLowerScope::Baseline`.

use foma::constructions::fsm_universal;
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

use crate::compose_budget::{compose_checked, union_checked, ComposeBudget, ComposeError};

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
    /// rules) — deliberately simple for now; alias-set size/rebuild cost is a `ComposeBudget`
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

    pub fn table(&self) -> &'t CharDefTable {
        self.table
    }
}

// Re-exported from `crate::lower` (canonical home) at the same paths so existing callers need no change.
pub(crate) use crate::lower::{pattern_slots, render_slots, resolve_alpha_tuples, Slot};
pub use crate::lower::{AlphaAssignment, TupleReport};

#[cfg(test)]
mod representation_alias_map_tests {
    //! Unit-level proof for `RepresentationAliasMap`/`SegAlphabet::render_tokens` --
    //! narrower and faster than the full
    //! grammar-level containment gate (`tests/two_table_shared_representation_recall.rs`), pinning
    //! the aliasing MECHANISM directly: the multimap's own contents, and `render_tokens`' union/
    //! degenerate-singleton contract.
    use super::*;

    fn two_table_shared_repr_grammar() -> Grammar {
        // Table A: "x" at index 0. Table B: "z"(0, decoy - misaligned index), "x"(1, shared), "y"(2).
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>AliasMapUnitProbe</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t0">
      <Name>TableA</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c0x"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <CharacterDefinitionTable id="t1">
      <Name>TableB</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1z"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c1x"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c1y"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
  </Language>
</HermitCrabInput>
"#;
        pg_grammar::load(XML).unwrap_or_else(|e| panic!("alias-map unit probe failed to load: {e}"))
    }

    /// "x" (shared) maps to both tables' pairs; "z"/"y" (unique to table B) map to their own single pair.
    #[test]
    fn build_maps_shared_representation_to_every_owning_table_and_cd() {
        let g = two_table_shared_repr_grammar();
        let map = RepresentationAliasMap::build(&g);

        let table_a = &g.char_tables[0];
        let table_b = &g.char_tables[1];
        let cd_a_x = table_a.lookup_nfd("x").expect("table A declares x");
        let cd_b_z = table_b.lookup_nfd("z").expect("table B declares z");
        let cd_b_x = table_b.lookup_nfd("x").expect("table B declares x");
        let cd_b_y = table_b.lookup_nfd("y").expect("table B declares y");
        // Sanity: "x" sits at different raw indices in the two tables -- why this fix is needed.
        assert_ne!(
            cd_a_x.0, cd_b_x.0,
            "the fixture's own misalignment must hold"
        );

        let aliases_x = map.aliases_for(table_b, TableId(1), cd_b_x);
        assert_eq!(
            aliases_x.len(),
            2,
            "\"x\" is shared by both tables -- aliases_for must return BOTH pairs: {aliases_x:?}"
        );
        assert!(aliases_x.contains(&(TableId(0), cd_a_x)));
        assert!(aliases_x.contains(&(TableId(1), cd_b_x)));

        // "z"/"y" are unique to table B: aliases_for degenerates to the singleton.
        assert_eq!(
            map.aliases_for(table_b, TableId(1), cd_b_z),
            vec![(TableId(1), cd_b_z)]
        );
        assert_eq!(
            map.aliases_for(table_b, TableId(1), cd_b_y),
            vec![(TableId(1), cd_b_y)]
        );
    }

    /// `SegAlphabet::new` never aliases, regardless of shared representations -- the byte-identical-to-today behavior existing parity tests depend on.
    #[test]
    fn render_tokens_never_aliases_without_a_table_id() {
        let g = two_table_shared_repr_grammar();
        let table_b = &g.char_tables[1];
        let cd_b_x = table_b.lookup_nfd("x").unwrap();
        let alphabet = SegAlphabet::new(table_b);
        assert_eq!(alphabet.render_tokens(cd_b_x), vec![alphabet.token(cd_b_x)]);
    }

    /// `with_table_id` renders the shared "x" atom as the union of both tables' tokens (deduplicated); unshared atoms degenerate to the single-token case.
    #[test]
    fn render_tokens_aliases_the_shared_atom_and_degenerates_for_the_unshared_ones() {
        let g = two_table_shared_repr_grammar();
        let table_a = &g.char_tables[0];
        let table_b = &g.char_tables[1];
        let cd_a_x = table_a.lookup_nfd("x").unwrap();
        let cd_b_z = table_b.lookup_nfd("z").unwrap();
        let cd_b_x = table_b.lookup_nfd("x").unwrap();
        let cd_b_y = table_b.lookup_nfd("y").unwrap();
        let map = RepresentationAliasMap::build(&g);
        let alphabet_b = SegAlphabet::with_table_id(table_b, TableId(1), &map);

        let alphabet_a_bare = SegAlphabet::new(table_a);
        let mut tokens_x = alphabet_b.render_tokens(cd_b_x);
        tokens_x.sort_unstable();
        let mut expected_x = vec![alphabet_b.token(cd_b_x), alphabet_a_bare.token(cd_a_x)];
        expected_x.sort_unstable();
        assert_eq!(
            tokens_x, expected_x,
            "the shared \"x\" atom must render as the union of BOTH tables' own tokens"
        );
        assert_eq!(
            tokens_x.len(),
            2,
            "must actually be two DISTINCT tokens, not a collapsed one"
        );

        assert_eq!(
            alphabet_b.render_tokens(cd_b_z),
            vec![alphabet_b.token(cd_b_z)]
        );
        assert_eq!(
            alphabet_b.render_tokens(cd_b_y),
            vec![alphabet_b.token(cd_b_y)]
        );
    }

    /// `encode_shape`/`encode_query` never alias, regardless of construction path -- a query word must stay single-token.
    #[test]
    fn encode_query_is_unaffected_by_table_id_aliasing() {
        let g = two_table_shared_repr_grammar();
        let table_b = &g.char_tables[1];
        let map = RepresentationAliasMap::build(&g);
        let bare = SegAlphabet::new(table_b);
        let aliased = SegAlphabet::with_table_id(table_b, TableId(1), &map);

        let bare_query = bare
            .encode_query("x")
            .expect("\"x\" must segment against table B");
        let aliased_query = aliased
            .encode_query("x")
            .expect("\"x\" must segment against table B");
        assert_eq!(
            bare_query, aliased_query,
            "encode_query must be byte-identical whether or not the alphabet carries a TableId"
        );
        assert_eq!(
            bare_query.chars().count(),
            1,
            "a single-segment query must encode to exactly one token, never a bracketed union"
        );
    }
}

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
/// construct (`Ok(None)`, reported `skipped` by its own caller); `capability.rs`'s
/// `lower_subrule_span` rounds it to `crate::capability::LoweredSpan::Unsupported` (any approximation rounds toward
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
/// behavior, unchanged for every existing caller). Builds a production `ComposeBudget` from
/// `HC_COMPOSE_*` env vars exactly once (mirrors `crate::emit::emit_with_precision`'s own
/// "read env in the production entry point only" convention) -- tests that need a deterministic,
/// tiny budget should call `compile_rewrite_rule_subset` directly with an explicit
/// `ComposeBudget::with_caps` instead.
pub fn compile_rewrite_rule(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    rule: &RewriteRuleDef,
) -> Result<Option<(Fsm, Vec<TupleReport>)>, ComposeError> {
    let budget = ComposeBudget::from_env();
    compile_rewrite_rule_subset(opts, g, alphabet, rule, &|_| true, &budget)
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
/// `budget`: checked at two points -- V3 immediately after `resolve_alpha_tuples` returns,
/// BEFORE the (potentially expensive) per-tuple compile loop runs (`AlphaTupleBudgetExceeded` if
/// `report.surviving` exceeds `ComposeBudget::tuple_cap`'s value, the same cheapest-possible-
/// predictor principle `EnumerationBudget` already uses); and V1, via `compose_checked`, on every
/// fold step of the per-alpha-tuple union-by-composition below.
///
/// **Mode/dir detection:**
/// `rule.mode`/`rule.dir` are checked FIRST, via `is_fully_supported_shape` -- a rule outside
/// that shape returns `Ok(None)` immediately, exactly the same "uncovered, caller reports it
/// `skipped`" contract `pattern_slots` already uses for an unsupported PATTERN construct (a
/// malformed `Quantifier` or a disagree-polarity alpha var -- cross-table and same-table
/// `Segments` plus any `Anchor` no longer disqualify a rewrite rule's own pattern at all, per this
/// function's own `PatternLowerScope::RewriteRuleCompile` call below). Before this check existed,
/// an unsupported mode/dir was silently
/// compiled via plain foma `->` as if it were Iterative/LeftToRight -- a WRONG network with no
/// signal ("silent mis-map"). `Dir::RightToLeft` used to be gated out here too
/// (`Ok(None)`, honestly skipped) until it gained
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
///
/// # `_alphabet` is unused
/// Every existing caller (this file's own `compile_and_compose_rules(_gated)_with_budget`, every
/// `tests/phase_c_*`/example driver) passes a single, grammar-wide `&SegAlphabet` built once
/// (typically `SegAlphabet::new(surface_table(g))`, the LAST stratum's table) and reused across
/// EVERY rule in the cascade, regardless of which table that rule actually owns. Since
/// `owning_table`/`owning_table_id` already resolve THIS rule's own correct table below, this
/// function now builds its OWN `SegAlphabet::with_table_id` (`render_alphabet`) for every render
/// call instead of trusting the caller's possibly-unrelated one — so the parameter is kept
/// (removing it would ripple through every one of those call sites, most outside this file's own
/// single-owner boundary) but no longer read. Renamed rather than silently unused to make that
/// explicit.
pub fn compile_rewrite_rule_subset(
    opts: &FomaOptions,
    g: &Grammar,
    _alphabet: &SegAlphabet,
    rule: &RewriteRuleDef,
    allowed: &dyn Fn(usize) -> bool,
    budget: &ComposeBudget,
) -> Result<Option<(Fsm, Vec<TupleReport>)>, ComposeError> {
    if !is_fully_supported_shape(g, rule) {
        return Ok(None);
    }
    // Resolved once per rule, never an implicit table-zero default; `None` is treated like an unsupported construct, reported `skipped` by callers.
    let Some(table) = owning_table(g, rule) else {
        return Ok(None);
    };
    // `owning_table_id` shares `owning_table`'s own stratum lookup, so it is guaranteed `Some` here too.
    let table_id = owning_table_id(g, rule)
        .expect("owning_table_id shares owning_table's own lookup, which just resolved Some");
    // Built once per rule rather than threaded from the outer cascade functions, so callers outside this file need no signature change.
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
        let Some(lhs_slots) = pattern_slots(g, table, &rule.lhs, &mut next_occurrence, scope)
        else {
            return Ok(None);
        };
        let Some(rhs_slots) = pattern_slots(g, table, &subrule.rhs, &mut next_occurrence, scope)
        else {
            return Ok(None);
        };
        let left_slots = match &subrule.left_env {
            Some(p) => match pattern_slots(g, table, p, &mut next_occurrence, scope) {
                Some(s) => s,
                None => return Ok(None),
            },
            None => Vec::new(),
        };
        let right_slots = match &subrule.right_env {
            Some(p) => match pattern_slots(g, table, p, &mut next_occurrence, scope) {
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
        // Checked before the per-tuple compile loop: the cheapest-possible predictor.
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
                &render_alphabet,
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
                // Sequential composition, not union -- see the per-subrule composition note above this loop.
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
/// Builds a production `ComposeBudget` from `HC_COMPOSE_*` env vars exactly once (mirrors
/// `crate::emit::emit_with_precision`'s own convention). Tests that need a deterministic, tiny
/// budget should call `compile_and_compose_rules_with_budget` directly instead.
///
/// Deliberately NOT given a final `minimize_checked` call (unlike `crate::gate::
/// compile_gated_grammar`): `tests/p6_gate_parity.rs`'s
/// `amharic_gated_subrules_and_tuple_counts_unregressed` hard-asserts this function's return value
/// is BYTE IDENTICAL to a fixed state/arc count (82 states / 1,110,358 arcs) with no minimize
/// applied by this function itself -- adding one here would change those counts (composing minimal
/// nets is not itself guaranteed minimal) and break that regression guard. Callers that want a
/// minimal composed rule net should call
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
) -> Result<Option<Fsm>, ComposeError> {
    let budget = ComposeBudget::from_env();
    compile_and_compose_rules_internal(
        opts,
        g,
        alphabet,
        prules_in_order,
        skipped,
        tuple_reports,
        &budget,
        true,
    )
}

/// `compile_and_compose_rules`'s core, with the `ComposeBudget` threaded in explicitly rather
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
    compile_and_compose_rules_internal(
        opts,
        g,
        alphabet,
        prules_in_order,
        skipped,
        tuple_reports,
        budget,
        false,
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
    budget: &ComposeBudget,
    optional_every_rule: bool,
) -> Result<Option<Fsm>, ComposeError> {
    let mut composed: Option<Fsm> = None;
    for pr in prules_in_order {
        let rule = match pr {
            PhonRuleDef::Rewrite(rule) => rule,
            PhonRuleDef::Metathesis(m) => {
                // A shape left honestly unsupported falls through to the same `skipped` report every unsupported construct uses, never a silent wrong compile.
                match compile_metathesis_rule(opts, g, alphabet, m, budget)? {
                    Some(net) => {
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
                    None => skipped.push(format!("{} (metathesis, unhandled)", m.xml_id)),
                }
                continue;
            }
        };
        // `is_fully_supported_shape` detects an unsupported RightToLeft/Simultaneous shape and reports it `skipped`, never a silent mis-map.
        match compile_rewrite_rule_subset(opts, g, alphabet, rule, &|_| true, budget)? {
            Some((net, reports)) => {
                tuple_reports.push((rule.xml_id.clone(), reports));
                let net = if optional_every_rule {
                    union_checked(
                        opts,
                        net,
                        fsm_universal(),
                        budget,
                        "compile_and_compose_rules recall-safe identity union",
                    )?
                } else {
                    net
                };
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

/// Identical to `compile_and_compose_rules`, but for ONE GATING GROUP (`crate::gate`): for every
/// `Rewrite`-kind rule at position `rule_pos` in `prules_in_order`, `subrule_ok(rule_pos, sub_idx)`
/// decides whether that specific subrule is included for THIS group (module doc: a group is a set
/// of lexical entries that agree on every gated subrule's applicability, so ungated subrules always
/// pass `subrule_ok` unconditionally — only `crate::gate`'s own gated-subrule list ever returns
/// `false`). A rule whose every subrule is filtered out for this group is skipped exactly like an
/// unsupported-construct rule (absent from the group's cascade, i.e. identity for this group) —
/// see `compile_rewrite_rule_subset`'s doc.
///
/// Builds a production `ComposeBudget` from `HC_COMPOSE_*` env vars exactly once -- same
/// convention as `compile_and_compose_rules`. Tests should call
/// `compile_and_compose_rules_gated_with_budget` directly instead.
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

/// `compile_and_compose_rules_gated`'s core, with the `ComposeBudget` threaded in explicitly
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
        let rule = match pr {
            PhonRuleDef::Rewrite(rule) => rule,
            PhonRuleDef::Metathesis(m) => {
                // A `MetathesisRuleDef` carries no subrules, so gating does not apply -- every group compiles the same metathesis relation.
                match compile_metathesis_rule(opts, g, alphabet, m, budget)? {
                    Some(net) => {
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
                    None => skipped.push(format!("{} (metathesis, unhandled)", m.xml_id)),
                }
                continue;
            }
        };
        // Mode/dir detection lives in `compile_rewrite_rule_subset` (`is_fully_supported_shape`), so an unsupported shape is reported `skipped`, never silently mis-compiled.
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
/// `Ok(None)` for it exactly like any other unsupported construct, honest-unsupported, never a
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
        Slot::ForeignFixed { .. } | Slot::Alpha { .. } | Slot::Repeat { .. } | Slot::Anchor(_) => {
            None
        }
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
    budget: &ComposeBudget,
    rule_xml_id: &str,
    table: &CharDefTable,
    table_id: TableId,
    aliases: &RepresentationAliasMap,
) -> Result<Option<Fsm>, ComposeError> {
    let leading_anchor = matches!(slots.first(), Some(Slot::Anchor(_)));
    let trailing_anchor = matches!(slots.last(), Some(Slot::Anchor(_)));
    let start = usize::from(leading_anchor);
    let end = slots.len().saturating_sub(usize::from(trailing_anchor));
    if slots[start..end]
        .iter()
        .any(|slot| matches!(slot, Slot::Anchor(_)))
        || left_idx < start
        || right_idx < start
        || left_idx >= end
        || right_idx >= end
    {
        return Ok(None);
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
            _ => return Ok(None), // Slot::Alpha/Slot::Repeat, or a vacuous empty class.
        }
    }

    // Checked before any regex is rendered or `Fsm` is built.
    let total: usize = candidates.iter().map(Vec::len).product();
    if total > budget.tuple_cap() {
        return Err(ComposeError::AlphaTupleBudgetExceeded {
            surviving: total,
            limit: budget.tuple_cap(),
            rule_xml_id: rule_xml_id.to_string(),
        });
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
            Some(prev) => union_checked(
                opts,
                prev,
                branch_net,
                budget,
                "compile_metathesis_rule cross-product union",
            )?,
        });
    }
    Ok(net)
}

/// The `Dir::RightToLeft` mirror's own switch indices: `(n - 1 - left_idx, n - 1 - right_idx)`. Factored out so `metathesis_mirror_switch_index_remap_tests` can pin the arithmetic without building a whole `Fsm`.
/// Derivation: `docs/research/pg-foma-replace-design-notes.md`, "Metathesis: the dedicated swap relation".
fn metathesis_mirror_switch_indices(n: usize, left_idx: usize, right_idx: usize) -> (usize, usize) {
    (n - 1 - left_idx, n - 1 - right_idx)
}

#[cfg(test)]
mod metathesis_mirror_switch_index_remap_tests {
    //! Pins `metathesis_mirror_switch_indices`'s exact arithmetic against an off-by-one in either
    //! direction (`n - left_idx` / `n - 2 - left_idx` instead of the correct `n - 1 - left_idx`) —
    //! see the module doc's "switch-index remap, worked out" derivation.
    use super::metathesis_mirror_switch_indices;

    /// Asymmetric placement, chosen so an off-by-one in either direction lands on a different, still in-bounds pair rather than masking a bug.
    #[test]
    fn asymmetric_five_slot_placement_matches_the_derived_formula_exactly() {
        let n = 5;
        let (left_idx, right_idx) = (0, 1);
        assert_eq!(
            metathesis_mirror_switch_indices(n, left_idx, right_idx),
            (4, 3),
            "correct remap: n - 1 - left_idx, n - 1 - right_idx"
        );
        // `n - left_idx` (no `-1`) would give (5, 4), out of bounds; `n - 2 - left_idx` would give (3, 2). Neither equals the correct (4, 3).
        assert_ne!(
            (n - left_idx, n - right_idx),
            (4, 3),
            "sanity: the n-left_idx off-by-one truly differs"
        );
        assert_ne!(
            (n - 2 - left_idx, n - 2 - right_idx),
            (4, 3),
            "sanity: the n-2-left_idx off-by-one truly differs"
        );
    }

    /// Documents (not a load-bearing witness) that `{0,3}` on a 4-slot pattern is its own mirror image under the correct formula, so alone it can't distinguish correct from off-by-one.
    #[test]
    fn four_slot_outer_placement_is_its_own_mirror_set_not_a_useful_off_by_one_witness() {
        let (mirror_left, mirror_right) = metathesis_mirror_switch_indices(4, 0, 3);
        let mut got = [mirror_left, mirror_right];
        got.sort_unstable();
        assert_eq!(
            got,
            [0, 3],
            "documented, not load-bearing: see this test's own doc"
        );
    }
}

/// Compiles one `MetathesisRuleDef` into the dedicated swap relation (module doc above), or
/// `Ok(None)` for a shape this change leaves honestly unsupported — the SAME "uncovered, caller
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
    budget: &ComposeBudget,
) -> Result<Option<Fsm>, ComposeError> {
    let Some(table) = owning_table_for_metathesis(g, rule) else {
        return Ok(None);
    };
    // Shares `owning_table_for_metathesis`'s own stratum lookup, so this is guaranteed `Some` here too.
    let table_id = owning_table_id_for_metathesis(g, rule)
        .expect("owning_table_id_for_metathesis shares owning_table_for_metathesis's own lookup, which just resolved Some");
    let alias_map = RepresentationAliasMap::build(g);
    let mut next_occurrence = 0usize;
    // Must stay in lockstep with `capability::metathesis_swap_construction_attempted`'s own scope, or the two could admit different rule sets.
    let scope = crate::lower::PatternLowerScope::RewriteRuleCompile;
    let Some(slots) = pattern_slots(g, table, &rule.pattern, &mut next_occurrence, scope) else {
        return Ok(None);
    };
    let left_idx = rule.left_switch as usize;
    let right_idx = rule.right_switch as usize;
    if left_idx == right_idx || left_idx >= slots.len() || right_idx >= slots.len() {
        // Defensive: both are always in bounds by construction; honest-unsupported rather than a panic if that ever changes.
        return Ok(None);
    }

    let Some(plain_net) = compile_metathesis_swap_net(
        opts,
        alphabet,
        &slots,
        left_idx,
        right_idx,
        budget,
        &rule.xml_id,
        table,
        table_id,
        &alias_map,
    )?
    else {
        return Ok(None);
    };

    match rule.dir {
        Dir::LeftToRight => Ok(Some(plain_net)),
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
                budget,
                &rule.xml_id,
                table,
                table_id,
                &alias_map,
            )?
            else {
                // Kept as an honest `Ok(None)` rather than `unreachable!` in case this equivalence is ever violated.
                return Ok(None);
            };
            let reversed_net = fsm_reverse(mirror_net);
            let unioned = union_checked(
                opts,
                plain_net,
                reversed_net,
                budget,
                "compile_metathesis_rule RTL safety-net union",
            )?;
            Ok(Some(unioned))
        }
    }
}

#[cfg(test)]
mod compose_budget_tests {
    //! This module's own test plan: a
    //! hand-authored, minimal grammar with ONE alpha-bound rewrite rule whose RHS occurrence draws
    //! from a natural class with a KNOWN, exact member count (6 -- an "Any"-style
    //! `FeatureNaturalClass` with zero explicit `FeatureValue` constraints of its own, so
    //! `class_members` returns every segment in the table; see `compile_rewrite_rule_subset`'s
    //! alpha-resolution doc). This gives `resolve_alpha_tuples` a `raw_product`/`surviving` of
    //! EXACTLY 6 by construction (a single occurrence trivially agrees with itself, module doc on
    //! `AlphaAssignment`), which every test below relies on.
    use super::*;

    /// 6 segments, all matching the sole natural class `ncBig` (an "Any" class); each carries a nonzero `featA` bit so alpha self-agreement passes.
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

    /// `resolve_alpha_tuples` surfaces exactly 6 surviving assignments; a `tuple_cap` below that must trip `AlphaTupleBudgetExceeded` before any per-tuple compile work.
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

    /// Proves the checked wrappers are pure passthrough when every cap is `usize::MAX` and `step_timeout` is `None`.
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

/// A positive witness for `owning_table`: two tables with different segment counts (2 vs 3), so an alpha-bound rule's `surviving` tuple count directly reveals which table it resolved against.
#[cfg(test)]
mod owning_table_tests {
    use super::*;
    use pg_grammar::model::PhonRuleDef;

    /// Table 0 (stratum "S0"): 2 segments. Table 1 (stratum "S1"): 3 segments -- deliberately different cardinalities so resolving against the wrong table gives a different, wrong `surviving` count.
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

    /// `owning_table` resolves `prule_alpha_t1` to table 1 (3 segments), never table 0 (2 segments).
    #[test]
    fn owning_table_resolves_to_the_rules_own_stratum_table_not_table_zero() {
        let g = two_table_alpha_grammar();
        assert_eq!(
            g.char_tables.len(),
            2,
            "fixture must declare exactly 2 tables"
        );
        assert_eq!(
            g.char_tables[0].len(),
            2,
            "table 0 must have exactly 2 segments"
        );
        assert_eq!(
            g.char_tables[1].len(),
            3,
            "table 1 must have exactly 3 segments"
        );
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

    /// Full compile-level proof: `resolve_alpha_tuples`'s own `surviving` count for `prule_alpha_t1` is exactly 3 (table 1's cardinality), never table 0's 2.
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

#[cfg(test)]
mod rtl_repeat_children_reversal_tests {
    //! Reproduces the shallow (pre-fix) `reversed_slots` shape compiling a wrong RTL branch net.
    //! See `docs/research/pg-foma-replace-design-notes.md`, "`reversed_slots`: why it must recurse into `Slot::Repeat` children".

    use std::collections::HashMap;

    use foma::apply::apply_init;

    use super::*;

    /// The pre-fix `reversed_slots` shape: a shallow reversal that never recurses into `Slot::Repeat` children. Kept only here as a negative witness.
    fn shallow_reversed_slots_pre_fix(slots: &[Slot]) -> Vec<Slot> {
        slots.iter().rev().cloned().collect()
    }

    /// Builds `fsm_reverse(mirror rule net)` for a rule whose only environment is `right_slots`, using whichever `reverse_fn` the caller supplies -- lets this test compare the shallow and fixed reversals in isolation (no union with `plain_net`).
    fn isolated_reversed_env_net(
        opts: &FomaOptions,
        alphabet: &SegAlphabet,
        lhs_slots: &[Slot],
        rhs_slots: &[Slot],
        right_slots: &[Slot],
        asg: &AlphaAssignment,
        reverse_fn: impl Fn(&[Slot]) -> Vec<Slot>,
    ) -> Fsm {
        let mirror_lhs = reverse_fn(lhs_slots);
        let mirror_rhs = reverse_fn(rhs_slots);
        // Swap: the mirror's own left environment is the reversed original right environment; this fixture has no left_env, so the mirror's right environment is empty.
        let mirror_left = reverse_fn(right_slots);
        let mirror_right: Vec<Slot> = Vec::new();
        let mirror_regex = render_branch_regex(
            alphabet,
            &mirror_lhs,
            &mirror_rhs,
            &mirror_left,
            &mirror_right,
            asg,
        );
        let mirror_net = fsm_parse_regex(opts, &mirror_regex, None, None)
            .unwrap_or_else(|| panic!("foma rejected mirror regex {mirror_regex:?}"));
        fsm_reverse(mirror_net)
    }

    /// Every `apply_down` result for `word` against `net`; this fixture's nets are deterministic per word, so a single-element `Vec` is expected throughout.
    fn apply_down_all(net: &Fsm, word: &str) -> Vec<String> {
        let mut h = apply_init(net);
        h.down(word).collect()
    }

    fn load(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    /// RTL rewrite rule `t -> d` gated by a right environment `(a b)^{1,max_attr}` -- two heterogeneous, non-palindromic children, so a correct reversal must swap them.
    fn rtl_hetero_repeat_xml(max_attr: &str) -> String {
        format!(
            r#"<HermitCrabInput><Language><Name>RtlHeteroRepeat</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cd"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prRtlHeteroRepeat" multipleApplicationOrder="rightToLeftIterative">
          <Name>rtlHeteroRepeatDemo</Name>
          <PhoneticInput><PhoneticSequence><Segment segment="ct" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><Segment segment="cd" /></PhoneticSequence></PhoneticOutput>
              <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence>
                <OptionalSegmentSequence min="1" max="{max_attr}">
                  <Segment segment="ca" /><Segment segment="cb" />
                </OptionalSegmentSequence>
              </PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
      <Strata><Stratum characterDefinitionTable="t1" phonologicalRules="prRtlHeteroRepeat"><Name>S</Name></Stratum></Strata>
    </Language></HermitCrabInput>"#
        )
    }

    /// Shared body: builds the isolated reversed-branch net both ways and demonstrates the divergence between the shallow and fixed reversal.
    fn reproduce_for_max_attr(max_attr: &str) {
        let g = load(&rtl_hetero_repeat_xml(max_attr));
        let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
            panic!("expected a Rewrite-kind rule");
        };
        assert_eq!(rule.dir, Dir::RightToLeft);
        let subrule = &rule.subrules[0];
        let right_env = subrule
            .right_env
            .as_ref()
            .expect("fixture declares a right environment");

        let table = owning_table(&g, rule).expect("rule is wired into stratum S's own cascade");
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();

        let mut next_occurrence = 0usize;
        let scope = crate::lower::PatternLowerScope::RewriteRuleCompile;
        let lhs_slots = pattern_slots(&g, table, &rule.lhs, &mut next_occurrence, scope)
            .expect("fixed-segment LHS must lower");
        let rhs_slots = pattern_slots(&g, table, &subrule.rhs, &mut next_occurrence, scope)
            .expect("fixed-segment RHS must lower");
        let right_slots = pattern_slots(&g, table, right_env, &mut next_occurrence, scope).expect(
            "a well-formed 2-child quantifier group must lower (bounded: \
             compile-bounded-fst-quantifiers; max=\"-1\": build-unbounded-quantifier-support)",
        );

        // Sanity: exactly one top-level `Slot::Repeat` with 2 heterogeneous children -- the shape a shallow reversal gets wrong.
        assert_eq!(right_slots.len(), 1);
        match &right_slots[0] {
            Slot::Repeat { children, .. } => {
                assert_eq!(
                    children.len(),
                    2,
                    "quantifier group must have exactly 2 children"
                );
            }
            _ => panic!("expected the right environment to lower to a single Slot::Repeat"),
        }

        let asg = AlphaAssignment {
            values: HashMap::new(),
        };

        // The crate's own (fixed, recursing) construction.
        let reversed_fixed = isolated_reversed_env_net(
            &opts,
            &alphabet,
            &lhs_slots,
            &rhs_slots,
            &right_slots,
            &asg,
            reversed_slots,
        );
        // The old, shallow, pre-fix construction, kept as a regression witness.
        let reversed_old_shallow = isolated_reversed_env_net(
            &opts,
            &alphabet,
            &lhs_slots,
            &rhs_slots,
            &right_slots,
            &asg,
            shallow_reversed_slots_pre_fix,
        );

        let query_tab = alphabet.encode_query("tab").expect("'tab' must segment");
        let query_tba = alphabet.encode_query("tba").expect("'tba' must segment");
        let query_dab = alphabet.encode_query("dab").expect("'dab' must segment");
        let query_dba = alphabet.encode_query("dba").expect("'dba' must segment");

        // The fixed (recursing) construction matches the rule's own stated environment: 't' followed by 'a' then 'b'.
        assert_eq!(
            apply_down_all(&reversed_fixed, &query_tab),
            vec![query_dab.clone()],
            "FIXED reversed_net: 't' followed by 'ab' satisfies the rule's own right_env -- must \
             rewrite (max={max_attr:?})"
        );
        assert_eq!(
            apply_down_all(&reversed_fixed, &query_tba),
            vec![query_tba.clone()],
            "FIXED reversed_net: 't' followed by 'ba' does NOT satisfy the rule's own right_env -- \
             must pass through unchanged (max={max_attr:?})"
        );

        // The old, shallow construction gets this backwards: it never reverses the 2 children, so it requires 'b' then 'a' -- the wrong environment.
        assert_eq!(
            apply_down_all(&reversed_old_shallow, &query_tab),
            vec![query_tab.clone()],
            "BUG (task #32, pre-fix): the shallow reversed_net does NOT rewrite 't' before 'ab' -- \
             it silently misses the rule's own real right-environment entirely, because a shallow \
             reversal never recurses into the Repeat's own children (max={max_attr:?})"
        );
        assert_eq!(
            apply_down_all(&reversed_old_shallow, &query_tba),
            vec![query_dba.clone()],
            "BUG (task #32, pre-fix): the shallow reversed_net WRONGLY rewrites 't' before 'ba' -- \
             the children order a shallow reversal leaves in DOCUMENT order instead of reversing it \
             (max={max_attr:?})"
        );

        assert_ne!(
            apply_down_all(&reversed_old_shallow, &query_tab),
            apply_down_all(&reversed_fixed, &query_tab),
            "the pre-fix shallow reversed_net's own compiled language genuinely differs from the \
             true reverse construction's -- task #32 REPRODUCED (max={max_attr:?})"
        );
    }

    /// Reproduction + regression pin, FINITELY bounded quantifier (`max="2"`).
    #[test]
    fn rtl_repeat_children_reversal_bug_reproduced_and_fixed_bounded() {
        reproduce_for_max_attr("2");
    }

    /// Same reproduction, genuinely unbounded quantifier (`max="-1"`): `max: None` hits the same defect and the same fix covers it.
    #[test]
    fn rtl_repeat_children_reversal_bug_reproduced_and_fixed_unbounded() {
        reproduce_for_max_attr("-1");
    }
}
