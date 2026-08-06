# pg-foma replace.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/src/replace.rs` and
`rust/crates/pg-foma/tests/phase_c_right_to_left.rs` implementation comments so the source can carry
a one- or two-line pointer instead of the full argument. Each section corresponds to one call site;
the site names the function/type so this doc can be found from either direction.

## `owning_table`: why table resolution must be threaded per rule, never defaulted

`pg_foma::replace::SegAlphabet::token` is a pure function of a `CharDefId`'s raw numeric index
(`PUA_BASE + cd.0`) with no awareness of which table that index belongs to. So any natural-class
resolution that silently defaults to table 0 rather than the rule's own owning stratum's table will
name whatever segment happens to sit at that same positional index in a *different* table — not the
linguistically intended one. Concretely observable failure: a phonological rule compiled for a
stratum whose own table is not table 0, resolved against table 0 and then converted into tokens via
the caller's (correctly table-1-scoped) alphabet, produces two deterministic wrong behaviors: a
voice+ root that never devoices, and a voice- root spuriously rewritten to the voice+ root's spelling.

Fix: `pattern_slots`/`resolve_alpha_tuples` take an explicit `&CharDefTable` parameter, and
`compile_rewrite_rule_subset` resolves it once per rule via `owning_table` (the rule's own stratum's
`StratumDef::table` — never an implicit table-zero default).

Worked example (`pg-foma/tests/phase_c_multi_table.rs`): two tables with deliberately misaligned
voice-feature-to-index assignment (table 0: voice+ at index 1, voice- at index 0; table 1: voice+ at
index 0, voice- at index 1). A rule on stratum 1 (table 1) doing an unconditional devoice rewrite
must resolve `ncVoicedAny`/`ncVoicelessAny` against table 1 itself, yielding table-1-local ids that
the same table-1-built alphabet converts consistently — never a cross-table reinterpretation. Single-
table grammars are byte-identical under this fix: every stratum's `table: TableId` is 0, so
`owning_table` always resolves to `g.char_tables[0]`, the exact value the old hardcoded default
returned (`tests/p6_gate_parity.rs`'s byte-exact Amharic regression guard pins this).

## `reversed_slots`: why it must recurse into `Slot::Repeat` children

A `Slot::Repeat{children, ..}` is not an atomic position the way `Slot::Fixed`/`Slot::Union`/
`Slot::Alpha` are — it is a variable-length sequence (each repetition contributes `children.len()`
tokens). Reversing the true flattened token sequence such a group can produce requires reversing
the order of tokens *within* each repetition too, not merely reordering which top-level slot comes
before which other one while leaving `children` in document order. A shallow `.iter().rev().cloned()`
gets this wrong whenever a `Repeat`'s own `children` has two or more heterogeneous entries:
reversing `[Fixed(y), Repeat{[a,b]}, Fixed(x)]` must give `[Fixed(x), Repeat{[b,a]}, Fixed(y)]` — the
group's own interior reversed too, not just its position among siblings.

Recursing is safe for the same reason the top-level reversal is: `Slot::Repeat`'s own `min`/`max`
bound is direction-invariant (reversing a language does not change how many times a repeated group
can occur), so only `children`'s own internal order needs to flip, including the `max: None`
(unbounded) case, which carries no numeric bound to touch at all.

This was reproduced concretely, not just argued: `rtl_repeat_children_reversal_tests` builds the
actual `Dir::RightToLeft` mirror-then-`fsm_reverse` construction twice — once via a shallow,
pre-fix `.rev().cloned()`, once via the real (recursing) `reversed_slots` — and runs `apply_down` on
concrete underlying strings through each resulting reversed net in isolation (before the safety-net
union with `plain_net`, which would let `plain_net` mask the divergence). On a synthetic RTL rule
`t -> d` gated by a right environment `(a b)^{1,max}` (two heterogeneous, non-palindromic children),
the two isolated nets disagree: the fixed net requires "a then b" (the rule's own stated order),
while the shallow one silently requires "b then a" — the exact swap the bug produces. This is
checked for both a finitely bounded (`max="2"`) and a genuinely unbounded (`max="-1"`) quantifier,
since widening `Slot::Repeat.max` to `Option<u32>` makes the unbounded case reachable by the same
code path.

## `render_branch_regex`: why empty LHS/RHS render as `"[..]"`/`"0"`, not a blank operand

foma's xre grammar rejects a literally blank LHS/RHS operand. Confirmed empirically by bisection:
`"0 -> x || a _ b"` silently compiles to a rule that never inserts on either tape, while `"[..] ->
x || a _ b"` — foma's own documented epenthesis notation (`foma::rewrite`'s test `rewrite_epenthesis`)
— behaves correctly. So an empty LHS renders as `"[..]"` and an empty RHS as `"0"`.

## `compile_rtl_branch_net`: worked example pinning the union's necessity

Plain `LeftToRight` compile of `"aa -> b"` prefers the leftmost non-overlapping match: on `"aaa"` it
yields `"ba"`. The mirror rule reverses to the same xre text (both LHS and RHS are palindromes here),
so `fsm_reverse` of the mirror compile yields a network that, on the same unreversed input `"aaa"`,
prefers the *rightmost* non-overlapping match instead: `"ab"`. `reversed_net` alone therefore
genuinely differs from `plain_net` on this input — proof the construction is not merely "compiled as
LeftToRight" — and the returned `fsm_union(plain_net, reversed_net)` accepts both `"ba"` and `"ab"`.
See `tests/phase_c_right_to_left.rs`'s `rtl-distinct-leftmost-rightmost` fixture for the pinned case.

No spurious third "did nothing" path is introduced by this union: both `plain_net` and
`reversed_net` are already complete, obligatory replace transducers over the full rule, so unioning
them only ever adds the second branch's own genuinely distinct rewrite, never a "nothing happened"
alternative.

## Per-subrule composition: two rejected constructions

Two approaches were tried and rejected before landing on sequential `fsm_compose` folding of
per-alpha-tuple branch nets:

1. Comma-joining full `LHS -> RHS || L _ R` branches in one regex string is rejected by this
   vendored xre grammar's parser whenever the branches don't share one RHS (this foma-rs's comma
   only joins multiple environments for a shared `LHS -> RHS`, or fully bare `LHS -> RHS` rules with
   no `||` at all — confirmed by direct bisection).
2. `fsm_union`-folding independently-compiled per-tuple nets is wrong, not just syntactically
   awkward: each per-tuple net is a complete replace transducer whose own "elsewhere" case is
   already identity. Unioning several such complete nets reintroduces a spurious "did nothing" path
   at positions where some *other* tuple's context obligatorily applies. Verified empirically:
   `apply_down` on a hand-built underlying string through the union returned both the correct path
   and a spurious unconverted-placeholder path.

The fix: since alpha tuples are, by the joint-agreement filter's own construction, mutually
exclusive at any one position, `fsm_compose`-folding them sequentially is correct — tuple K's net
only ever sees the placeholder if every earlier tuple left it untouched, and once any one tuple
rewrites it, no later tuple's (always-literal) LHS can match it again. This is the same "feeding
order" argument the outer stratum-level cascade already relies on, applied one level deeper.

## `rtl_anchor_reversal_swaps_the_correct_edge`: why a white-box proof, not a black-box comparison

An anchor pins a match to one word edge absolutely, so when it is the sole deciding factor a
correct compile of *either* declared direction recognizes the identical final language — both
correctly find the one true word-final occurrence. A black-box test that compiles a grammar as RTL
vs. as LTR and diffs the accepted languages would therefore see no difference for this shape even
for a correct implementation, and — worse — would also see no difference for a broken one that
silently no-ops the reversed branch. That style of test cannot catch a backwards anchor swap here.

The test instead compiles the reversed branch alone (mirror + `fsm_reverse`, no union with the plain
branch) and checks it independently computes the same word-final answer the plain branch does, then
adds a negative control: naively applying the swapped-but-unreversed hypothesis directly gives a
different, wrong answer (rewrites the word-initial `'a'` instead). That the naive hypothesis truly
differs is what proves the passing assertion is a real proof of the swap-then-reverse step, not a
coincidence of a construction that happens to be a no-op for this input.

## `rtl_segments_lhs_differs_from_left_to_right_at_the_fst_level`: a negative finding worth keeping

Comparing the *full* compiled nets' candidate sets for `Dir::LeftToRight` vs. `Dir::RightToLeft`
does not distinguish them for an unconstrained (no-environment) `"a a -> b"` rule: a plain,
unqualified replace rule is a genuinely nondeterministic transducer whose admitted relation already
contains both `"aaa:ba"` and `"aaa:ab"` pairs, even under the `LeftToRight` compile alone. So the
`RightToLeft` union adds nothing new at that level of observation — a real, useful negative finding
about this construction, not a bug to chase.

The genuine divergence lives one level down, at `apply_down`'s single-preferred-realization level
(the same level `rtl_distinct_leftmost_rightmost_...`'s bare-automaton half already establishes for
an ordinary `<SimpleContext>`-authored `"aa"`). A `Segments`-authored LHS renders to the identical
xre text as that ordinary-authored one, because `crate::lower::render_slots`'s `Slot::Fixed` arm
carries only the `CharDefId`, never whether it came from a `PatternNode::CharDef` or a
`PatternNode::Segments` node — so the existing, already-verified `apply_down` proof transfers here
verbatim rather than by assumption, and the test reproduces it directly instead of merely citing it.

## Metathesis: the dedicated swap relation

A `MetathesisRuleDef` is one match pattern plus two switch positions (`left_switch`/`right_switch`,
each a single index — the model has no way to represent a multi-node switch group). Synthesis swaps
the two switch nodes' own values in place: whichever is physically last in `pattern.nodes` ends up
first in the output and vice versa, with every node strictly between them keeping its own slot
(`pg_rules::metathesis::synthesis_reorder`'s `move_nodes_after`, cross-checked against
`machine/conformance/languages/metathesis-phase-isolation`'s `mrSimpleMeta`/`mrComplexMeta`, both of
which author `left_switch` physically after `right_switch`). This is tag-name-agnostic — it never
matters whether the physically-last node is tagged `leftSwitch` or `rightSwitch` — a literal
positional swap of the window's two endpoints, exactly the shape a plain foma `->` LHS/RHS
concatenation renders directly.

**The relation.** A switch may be a natural-class union with more than one member, and the value
that matched at one endpoint must reappear unchanged at its (possibly swapped) output position. A
plain `"[classA] ... [classB] -> [classB] ... [classA]"` rendering does *not* satisfy this: foma's
`->` builds a nondeterministic cross product of the two sides' languages, pairing any classA member
with any classB member rather than the one that actually matched — the same failure mode
`resolve_alpha_tuples` already found and fixed for alpha variables. `compile_metathesis_rule` applies
the same fix: resolve every slot's candidate members, enumerate the full cross product (no
joint-agreement filter is needed — metathesis has no shared-`VarId` constraint linking positions),
and for each concrete assignment render one fully-literal branch, transposing only the two switch
positions' tokens, then union every branch. This union is safe for the same reason
`compile_rtl_branch_net`'s safety-net union is: each branch is a complete, fully-literal transducer
with no identity escape hatch at a position its own literal context matches.

**Cross-table representation aliasing.** An ordinary rewrite rule aliases at render time
(`SegAlphabet::render_tokens`, a union over every table sharing a spelling). That shape is wrong
here, because the per-branch cross product's whole point is identity preservation between a matched
value and its (possibly swapped) output position: rendering both positions as independently-unioned
brackets would let foma's `->` pair either alias on the input with either alias on the output — an
input token from table A could emit table B's token at the swapped position, a correctness bug
strictly worse than the false negative being fixed. The fix instead pushes aliasing down into
`slot_candidates` itself: every slot's candidate set is expanded member-by-member to every `(table,
cd)` pair sharing that member's normalized representation, so the per-branch enumeration that keeps
the swap identity-preserving is untouched — each branch still fixes exactly one concrete `CharDefId`
per slot, just possibly drawn from another table. Pinned by
`tests/multi_table_metathesis_shared_representation.rs`'s
`current_compile_fires_on_table_a_originated_material_and_preserves_identity`.

**`Dir::RightToLeft`: the same mirror-and-reverse construction, and its index remap.** No
from-scratch RTL construction was needed — `compile_rtl_branch_net`'s reverse-mirror-then-
`fsm_reverse` technique operates on `Vec<Slot>` and `Fsm`, not rewrite-rule-specific data, so it
transfers directly. Since a `MetathesisRuleDef` pattern can never carry a `Slot::Repeat`/`Slot::Alpha`
occurrence (see below), every slot is atomic, so `reversed_slots` is a pure index reversal here:
index `i` in the `n`-slot original moves to index `n - 1 - i` in the mirror. Deriving the mirror's
own switch indices from the *original* switch indices `left_idx`/`right_idx` (not the swapped
pattern) requires care: for the swapped pattern `S` (original `P` with `lo`/`hi` transposed) and its
reverse `R(S)`, expressing `R(S)` as "`R(P)` with some pair of positions swapped" (the only shape
`compile_metathesis_swap_net` can build) works out to swapping positions `n - 1 - hi` and
`n - 1 - lo` of `R(P)` — i.e. `mirror_left_idx = n - 1 - left_idx`, `mirror_right_idx = n - 1 -
right_idx`, verified position-by-position. `metathesis_mirror_switch_index_remap_tests` pins this
arithmetic directly against both possible off-by-one errors (`n - left_idx` and `n - 2 - left_idx`),
independent of building a whole `Fsm`, since an index bug here is the single most likely defect in
this construction.

**Empirical finding: the confirm oracle is direction-blind for metathesis too.** Mirroring the
identical pre-fix finding for ordinary rewrite rules, a throwaway probe declared the same
two-adjacent-same-class-switch rule under both directions and called `pg_rules::metathesis::
synthesize` on an overlapping-window input: both directions synthesized identically, because
`match_candidates` sorts ascending regardless of `rule.dir` and the application loop always takes
the first (leftmost) candidate. So the safety-net union with the plain branch is the recall-safe
choice here too, not merely defensive — a theoretically-faithful reversal-only compile would
under-propose relative to what this repo's own confirm engine actually needs.

**Scope.** Faithful for either direction when: the rule resolves to a real owning table; the whole
pattern (both switches, every context node) has no `Slot::Alpha`/`Slot::Repeat` occurrence;
`left_switch != right_switch`; and the candidate cross product stays within `budget.tuple_cap()`.
`Slot::Alpha` is structurally impossible for a `<MetathesisRule>` — the loader gives it an empty
variable scope, so any `<AlphaVariable>` reference fails the whole grammar load before reaching this
code. `Slot::Repeat` is structurally reachable (the DTD allows an `OptionalSegmentSequence` in a
metathesis pattern) but never attested in any authored fixture; `slot_candidates` refuses it for
either direction. Disposition is `ConfirmOnly`, never `Admit`, for both directions: the per-branch
union is a proven superset, never proven exact, for the same reason RTL rewrite is `ConfirmOnly`.

## `tests/two_table_symbol_divergence.rs`: containment proof across two divergent tables

**What this proves, beyond `tests/phase_c_multi_table.rs`.** That file proves recall-via-compose for
one stratum's own rule. This one proves the stronger claim: two strata where the same symbol differs
between tables, each compiled rule uses its own table, and proposer-to-confirm results match the
oracle exactly (not merely a superset/subset) — using this codebase's established containment
methodology (`tests/f2_junction_gate.rs`'s `engine_sequences`/`candidates_cover`, `tests/f3_parity.rs`'s
"multiset parity" framing): decode every raw `apply_up` result off the compiled net into
`pg_foma::tags::Candidate`s and assert that set is exactly equal to `pg_parse::Morpher`'s own oracle
analysis set for the same surface word. `pg_rules::rewrite` already resolves every rule against its
real owning stratum's table via an explicit `TableId` parameter, so it is a trustworthy oracle for the
exact bug this module fixes: the proposer used to be the only table-zero-biased link in the chain.

**Why hand-authored XML, not `pg_grammar_gen`'s recipe generator.** `build::tables` always adds a
per-segment-unique `featId` feature to avoid `generate_words` surface collisions, and that defeats
`pg_parse::Morpher`'s un-apply of an environment-free feature-changing rewrite — a real, pre-existing,
unrelated characteristic of `pg-rules`' analysis engine (see the anomaly below, a different case of the
same class). This fixture sidesteps it by declaring only one phonological feature (`featVoice`) per
table.

**Scope: stratum 1 only.** A bare, unaffixed root declared on a non-final stratum with no
morphological rule bridging it forward is never a complete surface word in this architecture
(`pg_grammar_gen::build::strata`'s own module doc), so table 0 exists only to give the fixture two
strata each owning their own table, not to be queried against the oracle itself.

**Known, out-of-scope anomaly.** `pg_parse::Morpher`'s root lookup, run over the unfiltered whole
grammar, returns a third, spurious analysis for surface "k" naming stratum 0's own root ("p") — table
0's and table 1's segments happen to share the same raw per-table index (0), and `pg-parse`'s root
trie appears not to disambiguate cross-stratum/cross-table `CharDefId` identity the way
`pg_foma::replace::owning_table` now does for rewrite-rule compilation. This is a different component
(the root trie, not the rewrite compiler or engine) and a different bug class, flagged for a future
investigation rather than silently avoided. The oracle comparison in this test is restricted to
stratum 1's own two morphemes — the exact candidate universe the compiled net actually contains — so
this anomaly cannot contaminate the containment assertion.
