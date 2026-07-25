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
//! - [`PatternNode::Quantifier`] (`OptionalSegmentSequence`) whose own bound is genuinely UNBOUNDED
//!   (`max == None`, the DTD's `max="-1"` sentinel), inverted (`min > max`), pathologically large
//!   (past [`MAX_QUANTIFIER_BOUND`]), or carries an alpha-bound occurrence anywhere in its own
//!   children — [`pattern_slots`] still returns `None`/bails for exactly these configurations (a
//!   rule whose pattern needs one is reported uncovered, not silently mis-rendered). A FINITELY
//!   bounded, alpha-free quantifier (`min`/`max` both concrete, `min <= max <= MAX_QUANTIFIER_BOUND`)
//!   DOES compile now, via [`Slot::Repeat`] (`openspec/changes/compile-bounded-fst-quantifiers`) —
//!   see that variant's own doc for the construction, and "Bounded quantifiers" below for the
//!   compiled-vs-still-unsupported line and the confirm-engine finding that motivates it.
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
//!
//! ## Bounded quantifiers (`openspec/changes/compile-bounded-fst-quantifiers`; ADR 0001, `docs/adr/
//! 0001-honest-capability-boundary.md`)
//! [`PatternNode::Quantifier`] (`<OptionalSegmentSequence min max>`) used to be `pattern_slots`'
//! unconditional bail (module doc, "What this module does NOT attempt") regardless of `min`/`max`.
//! Now a FINITELY bounded, alpha-free quantifier — `max == Some(_)`, `min <= max <=
//! [MAX_QUANTIFIER_BOUND]`, no `Slot::Alpha` occurrence anywhere in its own (possibly nested)
//! children — compiles to a new [`Slot::Repeat`], rendered as foma's OWN native bounded-repetition
//! xre operator, `A^{min,max}` (`nfst-xre = "0.1.0"`'s `RepeatNToK`, confirmed by reading that
//! vendored crate's own `src/lexer.rs`/`src/parser.rs`: `^{N,K}`/`^N,K` lexes to `CatenateNToK`, a
//! POSTFIX operator over whatever `[...]`-grouped term precedes it) over the quantifier's own
//! rendered children — never a hand-rolled state-machine construction, so this file inherits
//! foma's own `fsm_concat_m_n` construction (`foma = "0.4.0"`'s own `src/constructions/boolean.rs`:
//! `min` mandatory concatenated copies of the child net, then `max - min` further copies each
//! wrapped in `fsm_optionality` — i.e. **exactly** the "bounded concatenation/optionality"
//! construction this change's own proposal names, not an approximation of it) for free. Genuinely
//! unbounded (`max == None`, the DTD's `max="-1"` Kleene sentinel), inverted (`min > max`, no sound
//! finite construction), over-[`MAX_QUANTIFIER_BOUND`], or alpha-nested quantifiers are UNCHANGED:
//! still `None`, still honestly reported uncovered by every existing caller — ADR 0001 forbids a
//! finite cutoff masquerading as unbounded Kleene semantics, so an out-of-scope config is never
//! silently rounded down to "the biggest bound we felt like compiling."
//!
//! **Big-O.** `A^{min,max}`'s compiled size is `O(max · |A|)` states/arcs (`max` sequential copies
//! of the child automaton `A`, `fsm_concat_m_n`'s own doc above) — LINEAR in the bound, never
//! exponential, and independent of `min` (a smaller `min` only changes how many of the `max` copies
//! are wrapped `fsm_optionality`-skippable, not how many copies exist). A rule combining a
//! quantifier with alpha variables ELSEWHERE in the same subrule (never inside the quantifier's own
//! children — disallowed, see [`Slot::Repeat`]'s own doc) multiplies this bound by
//! [`resolve_alpha_tuples`]'s own `surviving` tuple count, exactly the same two-independent-axes
//! shape [`ComposeBudget::tuple_cap`](crate::compose_budget::ComposeBudget::tuple_cap)'s own V3
//! check already guards on the alpha axis; the quantifier axis gets its OWN eager, cheaper-than-any-
//! `Fsm` preflight ([`MAX_QUANTIFIER_BOUND`], checked in `pattern_slots` before any regex is even
//! rendered, let alone parsed — the same "check the search result before the expensive part"
//! principle `docs/fst-plan/phase-b-compose-budget-design.md`'s V3 already uses for alpha tuples),
//! rather than a new [`crate::compose_budget::ComposeBudget`] dimension: `pattern_slots` is a pure
//! structural walk with no `ComposeBudget` threaded through it (every existing caller — this file's
//! own compile path, `crate::lower::lower_span`, `crate::capability`'s structural probes — calls it
//! with only a `&Grammar`/`&CharDefTable`), and widening that signature crate-wide for one
//! dimension's sake was judged a larger, separate follow-on rather than something this single-owner
//! slice should take on. **Residual, pre-existing, NOT introduced here**: the FIRST branch net of a
//! [`compile_rewrite_rule_subset`] fold (`net = None => branch_net` — no `compose_checked`/
//! `union_checked` call at all for that one net) already escapes every `ComposeBudget` size check
//! before the SECOND subrule's fold runs one — true for every existing construct this file compiles
//! (alpha tuples, RTL, Simultaneous), not a new gap a large quantifier bound opens; flagged here
//! because a large `max` is the most likely way any one branch net alone gets big, not because this
//! change caused it.
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

use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::reverse::fsm_reverse;
use foma::types::Fsm;

use pg_grammar::chardef::{CharDefId, CharDefTable};
use pg_grammar::model::{
    Dir, Grammar, MetathesisRuleDef, PhonRuleDef, PRuleId, RewriteMode, RewriteRuleDef,
    RewriteSubruleDef,
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
// `Slot`/`pattern_slots`/`resolve_alpha_tuples`/`render_slots`/`AlphaAssignment`/`TupleReport`
// MOVED to `crate::lower` (`openspec/changes/lower-fst-pattern-environments` Stage 1B migration
// follow-on, that module's own top doc: "What is reused vs. newly written vs. MOVED HERE").
// `crate::lower` is now the sole/canonical home for this pattern/environment-lowering vocabulary;
// this crate's own `pg-grammar` model is unaffected (`lower.rs`'s design.md: "frozen consumers").
// Re-exported here at the SAME paths/visibility they always had, so every existing caller in this
// file (`compile_rewrite_rule_subset`, `compile_rtl_branch_net`, `compile_metathesis_rule`,
// `slot_candidates`, `render_branch_regex`, `reversed_slots`) and every OUTSIDE caller
// (`capability.rs`'s structural probes -- a concurrent agent's exclusive file this change does not
// open; every `tests/phase_c_*` gate; every `pg_foma::replace::TupleReport`-importing example)
// keeps compiling completely unmodified -- see `crate::lower`'s own module doc for the full
// "what moved and why" account and this task's own final report for the byte-identical-output
// proof.
// =================================================================================================
pub(crate) use crate::lower::{pattern_slots, render_slots, resolve_alpha_tuples, Slot};
pub use crate::lower::{AlphaAssignment, TupleReport};

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
    owning_table_for_prule_position(g, idx)
}

/// [`owning_table`]'s sibling for a [`MetathesisRuleDef`] (`openspec/changes/compile-fst-metathesis`,
/// design.md/ADR 0001): identical reasoning, just matched against the `PhonRuleDef::Metathesis`
/// variant instead of `PhonRuleDef::Rewrite` — a `MetathesisRuleDef` lives in the SAME `g.prules`
/// vec and is wired to a stratum's own `prules: Vec<PRuleId>` list exactly the same way, so the
/// "find this rule's own index, then find which stratum's own cascade contains it" algorithm is
/// identical; only the variant match differs. Shares [`owning_table_for_prule_position`] with
/// [`owning_table`] rather than re-deriving the stratum lookup a second time.
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

/// Shared tail of [`owning_table`]/[`owning_table_for_metathesis`]: given a rule's own position
/// (`PRuleId`, found by the caller's own variant-specific `xml_id` lookup) in `g.prules`, find the
/// stratum whose `prules` list contains it and return that stratum's own `CharDefTable`. `None`
/// (never a panic, never an implicit table-zero guess) when no stratum's own `prules` list
/// contains it — see [`owning_table`]'s own doc for the full "unreachable from any stratum" case
/// this covers.
fn owning_table_for_prule_position(g: &Grammar, idx: usize) -> Option<&CharDefTable> {
    let target = PRuleId(idx as u32);
    let stratum = g.strata.iter().find(|s| s.prules.contains(&target))?;
    Some(&g.char_tables[stratum.table.0 as usize])
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
        let rule = match pr {
            PhonRuleDef::Rewrite(rule) => rule,
            PhonRuleDef::Metathesis(m) => {
                // `openspec/changes/compile-fst-metathesis`: attempt the dedicated swap relation
                // (module doc on `compile_metathesis_rule`) instead of an unconditional skip. A
                // shape this change leaves honestly unsupported (module doc's "Scope" section)
                // still falls through to the SAME `skipped` report every unsupported construct
                // uses, never a silent wrong compile.
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
        let rule = match pr {
            PhonRuleDef::Rewrite(rule) => rule,
            PhonRuleDef::Metathesis(m) => {
                // `openspec/changes/compile-fst-metathesis`: a `MetathesisRuleDef` carries no
                // subrules at all (no MPR/POS gating surface, module doc on `MetathesisRuleDef`),
                // so `subrule_ok`/gating simply does not apply -- every group compiles the SAME
                // metathesis relation `compile_and_compose_rules_with_budget` does.
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

// =================================================================================================
// Metathesis (`openspec/changes/compile-fst-metathesis`; ADR 0001, `docs/adr/
// 0001-honest-capability-boundary.md`): the dedicated swap relation.
//
// A [`MetathesisRuleDef`] (model.rs's own doc, cited there) is ONE match pattern plus two switch
// POSITIONS (`left_switch`/`right_switch`, each a single index into `pattern.nodes` — never a
// range: the model has no way to represent a multi-node switch group at all, unlike the oracle's
// own generalized `pg_rules::metathesis` machinery, which handles a possibly-wider *compiled
// segment* span defensively even though no DTD-legal, C#-loadable grammar can ever produce one
// wider than one node — see that module's own doc). Synthesis reorders the two switch nodes'
// OWN VALUES: whichever switch is PHYSICALLY LAST in `pattern.nodes` ends up FIRST in the output;
// whichever is physically first ends up last; every node strictly between them (if any) keeps its
// own slot, value unchanged (`pg_rules::metathesis::synthesis_reorder`'s own `move_nodes_after`
// algorithm, hand-traced and cross-checked against both of this repo's real HermitCrab-conformant
// fixtures — `machine/conformance/languages/metathesis-phase-isolation`'s `mrSimpleMeta`/
// `mrComplexMeta` — which both author `left_switch` physically AFTER `right_switch`, the only
// shape a genuine (non-vacuous) metathesis rule would ever use). This characterization is
// tag-name-agnostic (it never matters whether the physically-last node happens to be tagged
// `leftSwitch` or `rightSwitch`): it is a literal POSITIONAL swap of the window's two endpoint
// slots, with any interior/exterior context passed through unchanged — exactly the shape a plain
// foma `->` replace rule's own LHS/RHS concatenation can render directly, no new `foma`/`pg-fst`
// primitive needed (contrast [`compile_rtl_branch_net`]'s own reversal construction, which DOES
// need one).
//
// # The relation: per-branch literal cross product, unioned (mirrors `resolve_alpha_tuples`)
// A switch (or any other pattern position) may be a natural-class UNION with more than one member,
// and the value that matched at one endpoint must reappear, UNCHANGED, at its own (possibly
// swapped) output position — an identity-preservation requirement a plain "[classA] ... [classB]
// -> [classB] ... [classA]" rendering would NOT satisfy (foma's `->` builds a nondeterministic
// cross product of the two SIDES' own languages when they are not both singletons, silently
// pairing ANY classA member with ANY classB member rather than the one that actually matched at a
// given occurrence — exactly the failure mode `resolve_alpha_tuples`'s own module doc already
// found and fixed for alpha variables). [`compile_metathesis_rule`] applies the SAME fix: resolve
// every slot's own candidate members, enumerate the full cross product (no joint-agreement filter
// is needed here — unlike alpha variables, metathesis has no shared-`VarId` agreement constraint
// linking two positions; every combination is independently a real candidate underlying shape),
// and for each concrete assignment render ONE fully-literal branch — `"<pos0> <pos1> ... ->
// <pos0'> <pos1'> ..."`, document order, every non-switch position IDENTICAL text on both sides,
// the two switch positions' own concrete tokens TRANSPOSED — then union every branch together.
// This union is safe for the SAME reason [`compile_rtl_branch_net`]'s own safety-net union is
// (that function's own doc): each branch is a COMPLETE, fully-literal replace transducer with no
// "did nothing" escape hatch at a position its own (fully concrete, non-overlapping-by-construction
// — a real input position holds exactly one concrete token) literal context matches, so unioning
// adds no spurious identity path.
//
// # Scope this change compiles faithfully vs. leaves honestly unsupported
// **Faithful (`ConfirmOnly`, never `Admit` — ADR 0001, no proven no-false-negative admission-filter
// argument exists):** `Dir::LeftToRight` only; the rule resolves to a real owning
// [`CharDefTable`] ([`owning_table_for_metathesis`]); its WHOLE pattern (both switches, every
// interior/exterior context node) is a shape [`pattern_slots`] accepts with no [`Slot::Alpha`]/
// [`Slot::Repeat`] occurrence anywhere (no `AlphaVariable`/`OptionalSegmentSequence` is attested in
// ANY `<MetathesisRule>` this crate has ever seen — the DTD's own `<MetathesisRule>` structural
// description is a bare `<PhoneticSequence>` of `Segment`/`BoundaryMarker`/`SimpleContext` nodes,
// module doc on [`MetathesisRuleDef`] — so this is a conservative, evidence-based scope line, not
// an arbitrary one); `left_switch != right_switch` (the loader's own load-time invariant,
// defensively re-checked here); and the full slot-candidate cross product stays within
// `budget.tuple_cap()`.
// **Honest-unsupported (`Ok(None)`, reported `skipped` — never a silent wrong compile):**
// - `Dir::RightToLeft` — a genuine, tag-independent "prefer the rightmost overlapping match"
//   construction (mirroring [`compile_rtl_branch_net`]'s reverse-mirror-then-[`fsm_reverse`]
//   technique) is mechanically plausible here, but — unlike ordinary `Iterative` rewrite rules,
//   which `compile_rtl_branch_net`'s own doc found to be empirically DIRECTION-BLIND in
//   `pg_rules::rewrite` (justifying that function's safety-net union) — `pg_rules::metathesis`'s
//   own `compile_switch_pattern` genuinely THREADS `dir` into which physical end of an overlapping
//   candidate set wins (its own module doc's "Direction handling" section). A safety-net union
//   built on the WRONG assumption (oracle direction-blindness) would be unsound here, not merely
//   imprecise, so this change does not attempt it without its own dedicated oracle-matrix
//   containment witness (design.md task 1: "Enumerate switch, direction, ... variants" — not done
//   for `RightToLeft` in this pass). Flagged as a documented, evidence-based scope boundary for a
//   follow-on change, exactly like [`RewriteMode::Simultaneous`]/`Dir::RightToLeft` rewrite
//   compilation each shipped as their OWN separate `openspec` change before this one existed.
// - Any pattern needing `Quantifier`/`Segments`/`Anchor`/a disagree-polarity alpha var anywhere
//   (`pattern_slots`'s own scope line) — includes, notably, `finalBoundaryCondition`/
//   `initialBoundaryCondition` (`mrComplexMeta`'s own shape): an anchor lowers to a
//   `PatternNode::Anchor` node INSIDE `pattern.nodes` for a `MetathesisRuleDef` (`pg_grammar::load`'s
//   `load_metathesis_rule`), and `pattern_slots` refuses ANY `Anchor` occurrence grammar-wide today
//   (not a metathesis-specific gap — the identical refusal already applies to every
//   `RewriteRuleDef` LHS/RHS/environment carrying one).
// - A `Slot::Alpha`/`Slot::Repeat` occurrence anywhere in the pattern (not attested; see above).
// - No resolvable owning table ([`owning_table_for_metathesis`] returning `None`).
// - The slot-candidate cross product exceeds `budget.tuple_cap()` — reported via the SAME
//   [`ComposeError::AlphaTupleBudgetExceeded`] variant `compile_rewrite_rule_subset`'s own alpha-
//   tuple check uses (a deliberate reuse, not a new error variant: both are "how many concrete
//   per-position assignments must this rule's cascade fold over", the same shape, just driven by
//   slot-candidate cardinality here rather than `AlphaVar` occurrence cardinality).
//
// # Big-O
// `O(P · N)` states/arcs for the WHOLE rule, where `P` is the surviving cross-product size (bounded
// by `budget.tuple_cap()`, default 5,000 — module doc on [`ComposeBudget`]) and `N` is the
// pattern's own node count (each branch is one small literal-concatenation `Fsm`, `fsm_union`-folded
// `P` times) — linear in both, the same shape [`resolve_alpha_tuples`]'s own per-tuple fold already
// has, never exponential.
// =================================================================================================

/// Every [`CharDefId`] a slot may concretely resolve to, in this rule's own document order — `None`
/// for [`Slot::Alpha`]/[`Slot::Repeat`] (out of scope for metathesis, module doc above: not attested
/// in any `<MetathesisRule>` this crate has seen).
fn slot_candidates(slot: &Slot) -> Option<Vec<CharDefId>> {
    match slot {
        Slot::Fixed(cd) => Some(vec![*cd]),
        Slot::Union(members) => Some(members.clone()),
        Slot::Alpha { .. } | Slot::Repeat { .. } => None,
    }
}

/// Compiles one [`MetathesisRuleDef`] into the dedicated swap relation (module doc above), or
/// `Ok(None)` for a shape this change leaves honestly unsupported — the SAME "uncovered, caller
/// reports it `skipped`" contract [`compile_rewrite_rule_subset`] already uses for an unsupported
/// `RewriteRuleDef` pattern construct.
pub(crate) fn compile_metathesis_rule(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    rule: &MetathesisRuleDef,
    budget: &ComposeBudget,
) -> Result<Option<Fsm>, ComposeError> {
    // `Dir::RightToLeft`: honestly unsupported (module doc's own "Scope" section) — never silently
    // compiled as if `LeftToRight`.
    if !matches!(rule.dir, Dir::LeftToRight) {
        return Ok(None);
    }
    let Some(table) = owning_table_for_metathesis(g, rule) else {
        return Ok(None);
    };
    let mut next_occurrence = 0usize;
    let Some(slots) = pattern_slots(g, table, &rule.pattern, &mut next_occurrence) else {
        return Ok(None);
    };
    let left_idx = rule.left_switch as usize;
    let right_idx = rule.right_switch as usize;
    if left_idx == right_idx || left_idx >= slots.len() || right_idx >= slots.len() {
        // Defensive only: the loader's own `load_metathesis_rule` already refuses to build a
        // `MetathesisRuleDef` with `left_switch == right_switch`, and both indices are resolved
        // from `pattern.nodes` itself, so both are always in bounds by construction. Never reached
        // by any grammar this crate's own loader can produce; honest-unsupported rather than a
        // panic if that invariant is ever violated some other way.
        return Ok(None);
    }
    let (lo, hi) = (left_idx.min(right_idx), left_idx.max(right_idx));

    let mut candidates: Vec<Vec<CharDefId>> = Vec::with_capacity(slots.len());
    for slot in &slots {
        match slot_candidates(slot) {
            Some(members) if !members.is_empty() => candidates.push(members),
            _ => return Ok(None), // Slot::Alpha/Slot::Repeat, or a vacuous empty class.
        }
    }

    // Cheapest-possible-predictor check (module doc's Big-O note; mirrors `resolve_alpha_tuples`'s
    // own V3 discipline): computed BEFORE any regex is rendered or `Fsm` is built.
    let total: usize = candidates.iter().map(Vec::len).product();
    if total > budget.tuple_cap() {
        return Err(ComposeError::AlphaTupleBudgetExceeded {
            surviving: total,
            limit: budget.tuple_cap(),
            rule_xml_id: rule.xml_id.clone(),
        });
    }

    // Full cross product, no joint-agreement filter (module doc: metathesis has no shared-`VarId`
    // agreement constraint between positions — every combination is independently a real candidate
    // underlying shape).
    let mut assignments: Vec<Vec<CharDefId>> = vec![Vec::with_capacity(slots.len())];
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
            panic!("foma rejected compiled metathesis regex for rule {}: {regex:?}", rule.xml_id)
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
