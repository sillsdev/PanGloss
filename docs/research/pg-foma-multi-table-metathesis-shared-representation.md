# Multi-table metathesis with shared representation (`tests/multi_table_metathesis_shared_representation.rs`)

Closes a residual gap in cross-table representation aliasing:
`crate::replace::compile_metathesis_swap_net` used to render every switch-position token directly
(`SegAlphabet::token`, table-blind, no cross-table alias expansion) instead of through the
alias-expanded path `RepresentationAliasMap`/`SegAlphabet::render_tokens` gives ordinary rewrite
rules (`tests/two_table_shared_representation_recall.rs`). Fixture:
`conformance-staging/edge-cases/multi-table-metathesis-shared-representation` — see its
`STAGING.md` for the "second, separate discovered finding" section referenced below.

## The fix: alias-expand `slot_candidates`, never text-union the swap

`crate::replace::slot_candidates` now expands every member `CharDefId` to every `(table, cd)` pair
sharing its normalized representation (see `pg_foma::replace`'s module doc, "Cross-table
representation aliasing" section, for the full derivation) — NOT by rendering a bracketed union at
each position the way `crate::lower::render_slots` does for ordinary rewrite rules. A text-level
union would be UNSAFE here: `compile_metathesis_swap_net`'s per-branch construction requires the
swap to reproduce the EXACT SAME value that matched at its (possibly swapped) output position, and
independently unioning LHS/RHS at one position would let the compiled transducer pair a matched
alias with a DIFFERENT alias's token — a new correctness bug strictly worse than the false
negative being fixed. Since each cross-product branch fixes ONE concrete `CharDefId` per position
and the swap only permutes that same literal assignment vector (`rhs_vals.swap(lo, hi)`),
switch-position identity holds by the same argument the pre-existing per-branch construction
already relies on for ordinary (non-aliased) multi-member natural classes
(`tests/phase_c_metathesis.rs`'s `metathesis_multi_member_classes_transpose_precisely_not_naively`)
— extended one level: "candidate member" now ranges over aliased `(table, cd)` pairs, not only
this table's own char-defs.

## A structural, pre-existing over-approximation, observed and worked around rather than hidden

`fsm_union`-ing two or more complete per-branch replace nets means that whenever one branch's
literal LHS does NOT match a given input anywhere, that branch's net treats the whole input as
ordinary replace-rule "elsewhere" context and passes it through UNCHANGED — a valid path through
the union net, independent of any other branch's genuine rewrite. So applying the union to an
input that matches EXACTLY ONE branch yields TWO paths: that branch's real swap, and every other
non-matching branch's pure-identity pass-through. This is a property of ANY multi-branch
metathesis construction — it already exists, unexercised, for `tests/phase_c_metathesis.rs`'s
`MULTI_MEMBER_XML` fixture, which has 4 branches and never checks the FST's behavior on its own
raw, un-swapped surface. This task's aliasing fix did not introduce it; it just means more
branches (aliased ones too) each potentially contribute their own identity alternative. Safe under
propose-and-confirm (an EXTRA candidate the oracle/confirm engine prunes, never a missing one), so
every assertion in this file's tests checks CONTAINS/subset, never exact `Vec` equality, to avoid
conflating this pre-existing, harmless noise with the actual claim (recall + no wrong-alias
substitution).

## Proven in four steps, mirroring `two_table_shared_representation_recall.rs`'s methodology

1. **The loss is real.** A hand-rendered, pre-fix-equivalent swap net never fires when fed a token
   drawn from a different table's raw index for the same spelling.
2. **The fix closes it.** The same rule, compiled via the current (fixed)
   `compile_and_compose_rules_with_budget`, DOES fire on that exact material.
3. **Switch-position identity holds under aliasing.** Feeding every combination of aliased and
   non-aliased candidates at the two switch positions, the swap always reproduces exactly the
   matched values at their transposed positions.
4. **Containment holds end to end** for every word this fixture's oracle can actually analyze —
   see below for why ROOT1's cross-table word is excluded from that specific comparison.

## A separate, out-of-scope oracle finding this file does NOT work around

`pg_parse::Morpher` itself (via `pg_rules::metathesis`/`pg_rules::bridge`, never
`pg_foma::replace`) currently finds ZERO analyses for ROOT1's correctly-metathesized surface
("xm"), for a reason narrowed to raw-index misalignment but not root-caused within this task's
`pg-foma`-only boundary — orthogonal to, and not fixed by, this fix. Consequently:

- The standard "FST propose+decode set EQUALS the oracle set" Stage-2 containment shape is pinned
  for ROOT2 (same-table, oracle succeeds) — a genuine, non-vacuous check.
- For ROOT1 (cross-table), a full oracle-equality comparison would be VACUOUSLY true (the oracle's
  set is empty for reasons unrelated to this fix) and would prove nothing. Instead,
  `current_compile_fires_on_table_a_originated_material_and_preserves_identity` and
  `fst_proposes_root1_for_its_correctly_metathesized_surface` demonstrate the actual claim this
  task is responsible for — the FST proposer's own recall — directly against the compiled net.
