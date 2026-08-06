# Rewrite-rule port divergences (`pg-parse/tests/csharp_port_rewrite.rs`)

Findings from porting `RewriteRuleTests` from the C# HermitCrab test suite
(`tests/SIL.Machine.Morphology.HermitCrab.Tests/PhonologicalRules/RewriteRuleTests.cs`). Scope
notes: `MergeRules`/`MultipleMergeRules`/`ExpandRules` are out of scope (analysis-side depends on
work not yet landed elsewhere). `EpenthesisRules` omits the `RightToLeft` + left-anchor
infinite-loop-detection negative case (no loop/budget detection exists in `pg_rules::rewrite`, so
it would hang) and the alpha-variable agreement reconfiguration (covered elsewhere, via
`pg-rules/tests/alpha_gate.rs`). `DeletionRules` omits the `Morpher.DeletionReapplications = 1`
reconfiguration (no equivalent knob on `pg_parse::Morpher`'s public API).

## The char_def-staleness bug (module-level root cause)

`pg_rules::rewrite::ana_feature` (`pg-rules/src/rewrite.rs`) correctly widens a changed feature's
lanes to full-mask on analysis-unapply (the documented "underspecify on unapply" behavior) but
never touched the node's `char_def`/`cd_set`. Confirmed via direct calls: unapplying a rule that
changes "v" back toward "p" produced a node whose lanes were correctly widened but whose
`char_def` was still literally "v"'s. Root-allomorph lookup
(`pg_parse::root_trie::RootAllomorphIndex::search`) keys off that literal `char_def`, so it
returned zero matches for the widened-but-still-"v" node even though a "p"-rooted lexical entry
exists and the node's lanes are lane-compatible with "p" — an analysis-side reconstruction could
never find a lexical root whose underlying segment differs from the word's own surface segment at
that position.

Fix: `ana_feature` now clears the changed node's `char_def` to `NO_CHAR_DEF` after widening its
lanes, mirroring `syn_feature`'s pre-existing identical clearing, so root lookup falls back to pure
lane unification — matching C#'s own always-lane-based `CharacterDefinitionTable.GetMatchingStrReps`.
`ana_narrow`/`ana_epenthesis` needed no change (see the commit that landed this fix for the
per-function reasoning). This crate's `MutNode` (`rewrite.rs`) carries no separate `cd_set` column,
unlike `pg-rules/src/morph.rs`'s `OutNode`, which needed the fuller `ctx_cd_set`-based fix (see
`csharp_port_affix_process.rs`'s `ModifyFromInput` findings).

This fixed `common_feature_rules` and the `boundary_rules` sub-cases that depended only on
char_def staleness. `anchor_rules`, `boundary_rules`' remaining sub-cases, and the epenthesis tests
below each turned out to have separate, deeper root causes not reached by this fix.

## `anchor_rules`: root lookup needs unification, not char-def identity, on feature-bearing tables

Sub-case (1) failed missing root "10": that root's allomorph is `"ga̘p"` (ATR-, a distinct
`char_def` from surface "gap"'s plain "a") — two different concrete char-defs that the rule under
test never touches, so neither is ever `NO_CHAR_DEF`. The apparent fix looked like it needed a
multi-table/cross-stratum redesign, but the real root cause was narrower: C#'s root lookup is pure
`FeatureStruct.IsUnifiable` with no separate char-def-identity gate at all —
`CharacterDefinitionTable.Add` only attaches a `StrRep` disjunction when the segment has zero
authored phonological features (e.g. Sena); a feature-bearing segment (this fixture's Indonesian/
Amharic-style segments) gets `Type + features` and no `StrRep` at all. So two distinct concrete
char-defs whose feature structs unify legitimately cross-match root lookup in C# even within one
table.

Fix: `pg_grammar::chardef::CharDefTable` precomputes a build-time unifiability closure over a
feature-bearing table's segment char-defs (`unif_closure`/`unifiable_cds`, gated on
`!PhonFeatureSystem::is_empty()` so zero-feature grammars like Sena are untouched);
`root_trie::edge_matches`'s concrete×concrete arm and `surface::matching_reps_for_node`'s
concrete-identity gate both fall back to that closure on an equality miss, restoring C#'s
two-regime semantics (identity where C# has `StrRep`, unification where it doesn't) at both the
trie and synthesis-confirm sites.

## `boundary_rules`: two separate bugs in epenthesis at a word boundary

Sub-cases (5)/(6) (MPR-gated epenthesis) returned empty even with the MPR gating stripped
entirely — independent of char_def staleness. Two bugs, found by direct instrumentation:

1. **Missing word-initial synthesis site.** `pg_rules::rewrite::syn_epenthesis`'s site
   enumeration loop (`for (site, &node) in node_of.iter().enumerate()` with
   `left_end = right_start = site + 1`) only ever considers the gap *after* each existing segment
   — the word-initial gap before segment 0 is never a candidate site, so an epenthesis whose
   environment holds only at position 0 can never fire during synthesis. C#'s
   `SynthesisRewriteRuleSpec`'s pattern walk starts before the first segment annotation, so
   position 0 is an ordinary application site there. Fixed by adding the site-0 gap (splice after
   `ms.nodes[0]`, the left anchor), the synthesis twin of `ana_narrow_deletion`'s already-landed
   fix. Unit gate: `pg-rules/tests/rewrite_gate.rs::epenthesis_synthesis_word_initial_site`.
2. **Analysis-side direction inversion for multi-node targets.** This rule's RHS is 2 nodes, and
   `compile_lane_fst` compiled multi-node analysis targets in document order for a `RightToLeft`
   traversal — under `pg_fst`'s "nodes follow traversal order" convention, that matched the
   physically reversed sequence, so `ana_epenthesis` never marked the target nodes optional and no
   candidate root ever reached synthesis-confirm. C#'s `PatternNode.GenerateNfa` enumerates
   children in `fsa.Direction` order, so an RTL matcher matches the same physical substring as
   LTR. Fixed by reordering document→traversal inside `compile_lane_fst`; invisible on every
   reference grammar's single-node analysis targets. Unit gate: `rewrite_gate.rs::
   epenthesis_analysis_multi_node_target_matches_document_order`.

Oracle fixture: `rust/conformance/rewrite/word-initial-epenthesis/`.

`boundary_rules_required_pos_on_subrule_finding`'s POS gate (`requiredPartsOfSpeech` on a
subrule) is real and correct on its own, but was confounded by the same bare-root epenthesis gap
above (a bare-root, no-morphological-rule epenthesis-only phonological rule never got re-applied
on synthesis-confirm at all, independent of any gating condition). Once boundary_rules' word-
initial-site + multi-node-direction fixes landed, the POS gate composes correctly: `taba` resolves
to `pos2` only, `ba` to `pos1` only.

## `epenthesis_rules`: three findings behind one symptom

Sub-case (7) (`"biiibuii" -> "18"`) was a fixture bug: the shared lexicon's root "18" entry stored
the wrong shape ("bibabi" instead of "bibu"), confirmed against `HermitCrabTestBase.cs:565`
(`AddEntry("18", ..., Allophonic, "bibu")`). Fixed at the fixture, not the engine.

Sub-cases (2) and (5) (epenthesis adjacent to root "19"'s own internal morpheme boundary)
revealed three distinct mechanisms, only the third of which was decisive for the symptom:

1. **Real but not decisive on its own:** `pg_rules::bridge::PatternBridge::nat_class_lanes`'s
   `NaturalClassKind::Feature` arm never pinned the synthetic `Type` lane, unlike C#'s
   `NaturalClass` ctor, which unconditionally stamps it (`NaturalClass.cs:9-13`). Fixed by pinning
   `lanes[type_flat] = TYPE_SEGMENT_BITS` there too.
2. **Not a bug.** `pg_fst::traverse::Transduce::initialize`'s `start_anchor && optional` skip-arm
   faithfully ports C#'s `TraversalMethodBase.Initialize` (`TraversalMethodBase.cs:203-222`), which
   has the identical "an anchored match may transparently skip a leading Optional annotation"
   behavior — deliberate and shared, not a Rust-only overreach.
3. **The decisive bug:** `syn_epenthesis`'s outer site-enumeration loop treated a `Boundary`
   node's own `node_of` slot as a candidate epenthesis site. C#'s empty-LHS pattern is a single
   `Segment`-or-`Anchor` constraint (`SynthesisRewriteRuleSpec.cs:26-29`), never `Boundary` — a
   boundary is only ever traversed transparently within an environment check (mechanism 2), never
   itself a match position. Rust's `node_of` includes boundaries (needed so environment checks can
   see through them), and the site loop iterated every entry including boundary ones, double-
   counting root 19's boundary and manufacturing a second, C#-nonexistent site. Fixed by skipping
   `NodeKind::Boundary` entries in `syn_epenthesis`'s site loop.

A ninth sub-case, believed passing individually, turned out to be a separate divergence — see
"Iterative epenthesis cascading" below.

## Iterative epenthesis cascading is unimplemented (open, `#[ignore]`d)

`RewriteRuleTests.EpenthesisRules`' last reconfiguration (two bare `Iterative`-mode rules composed
in one stratum) fails: `m.parse_word("butubu")` returns empty against the C# oracle's `{"25"}`
(re-verified directly against `dotnet test`, so this is a genuine Rust divergence).

Root cause: `pg_rules::rewrite::syn_epenthesis` collects every candidate site against one
unmutated snapshot of the shape and splices all accepted sites in unconditionally, regardless of
the rule's declared `RewriteMode` — it is structurally Simultaneous-shaped even when a rule asks
for `Iterative` semantics. C#'s real `IterativePhonologicalPatternRule` finds one match, applies it
(mutating the live `Word`), and only then looks for the next match against the partially-rewritten
shape. On root 25's shape, `rule1` alone produces two insertions correctly, but `rule2`'s `[V]_[V]`
environment then finds three separate V-V adjacencies in the resulting 7-segment intermediate shape
(each of `rule1`'s freshly-inserted vowel nodes creates a new adjacent pair) and inserts at all
three, producing a shape that no longer matches the expected surface — where C#'s true iterative
cursor, which never re-visits a position it has already advanced past, would only accept a subset.

Deliberately not fixed: making `syn_epenthesis` faithfully iterative is a substantially larger,
separate rewrite of the epenthesis synthesis path that every other epenthesis reconfiguration in
this file depends on, and risks regressing the sub-cases that do pass. Left `#[ignore]`d in
`epenthesis_rules_iterative_cascade_finding`, split out so the rest of `epenthesis_rules` can gate
on its own.

## `multiple_segment_rules_deletion_composition_finding`: analysis-target span vs. an interposed Optional

Adding a pure-deletion rule (`rule2`, never actually fires on the test word) to the same stratum as
a 2-segment rewrite rule (`rule1`) made the whole composition lose every candidate, even though
each rule analyzes correctly alone.

Real root cause: `rule1`'s analysis target match (`ana_feature`) recovered each target-pattern
row's matched segment via a positional `node_of[s..e]` slice of the overall match span.
`pg_fst::traverse::Transduce::advance`'s "skip the next Optional annotation" mechanism (needed so a
2-segment target can transparently pass over `rule2`'s newly-interposed Optional "t")
reports every such match as a span *wider* than the pattern, and since no alternative exactly-
2-wide match exists either (every candidate site has an Optional immediately inside the pair), the
pre-existing `width_matches` guard — written assuming a tight duplicate always survives alongside a
wide one — discarded every candidate.

Fix: `ana_feature`'s target FST now compiles each target-pattern row in its own named
`CompileNode::Group` (`compile_lane_fst_grouped`, mirroring C#'s
`FeatureAnalysisRewriteRuleSpec.cs:48,68-71` `Group` mechanism) and reads each row's matched
segment from that group's own tag, recovering the correct per-row position regardless of
interposed Optional segments — `width_matches` is no longer needed at this call site. Empirically,
which tag half (start vs. end) is trustworthy is direction-dependent: `LeftToRight` targets must
read each row's start (an entering tag is always freshly computed; only a row's end can be widened
by a *following* skip), while `RightToLeft` targets (analysis always compiles the reversed
direction) must read each row's end instead, because the compiled node order is document-reversed
and `Fst::get_offsets` swaps `(start,end)` back for that direction. `resolve_bindings`/
`pattern_defaults_ok` were generalized from an implicit `node_of[s+k]` contiguity assumption to an
explicit `target_nodes: &[usize]` parameter so they work for both the old contiguous-slice callers
and `ana_feature`'s new non-contiguous list.

## `multiple_application_rules`: `RewriteMode::Simultaneous` needed real synthesis semantics

This test's point is that `Simultaneous` and `Iterative` produce different results on the same
rule over overlapping-match input. `RewriteMode::Simultaneous` used to be parsed but silently
executed identically to `Iterative`, then later made to hard-fail at grammar-load time instead.
Both gaps are now closed: `Simultaneous` loads and has real synthesis semantics
(`pg_rules::rewrite::sim_feature`) — `synthesize_with_mpr`/`synthesize_with_mpr_cached` dispatch on
`(classify(rule, sr), rule.mode)`, collecting every accepted match against one pristine snapshot
before applying any of them (mirroring C#'s `SimultaneousPhonologicalPatternRule.Apply` exactly),
instead of `syn_feature`'s find-one-then-rescan Iterative shape. Oracle-verified via the
`rewrite/simultaneous-feeding`/`simultaneous-feeding-control-iterative` conformance fixtures.

## `deletion_rules_multi_position_reinsertion`: reinsertion is a single pass, not enumerated

C# does not run an iterative power-set search over reinsertion sites; it runs a single analysis
pass whose optional-insert annotations get expanded combinatorially downstream, at root lookup.
`AnalysisRewriteRule.Apply`'s Deletion branch runs exactly `1 + Morpher.DeletionReapplications`
passes, and `DeletionReapplications` defaults to 0 — every reconfiguration ported here uses a
default `Morpher`, so the gold expectations come from a single analysis pass. Within that pass,
`SimultaneousPhonologicalPatternRule.Apply` collects all matches and applies every one, and
`NarrowAnalysisRewriteRuleSpec.Unapply` re-inserts each deleted segment as an **optional** node.
The "power set of reinsertion subsets" is realized downstream at root lookup: the FST traversal
may consume or skip each optional annotation independently, so one optional-decorated shape
reaches all four gold roots with no rule-level enumeration at all. Rust's `ana_narrow_deletion`
implements exactly this shape (all-sites-in-one-pass optional inserts, with skip-or-consume
branching in `pg_parse::root_trie::search_segs_opt`). Oracle fixture:
`rust/conformance/rewrite/deletion-reinsertion/`.
