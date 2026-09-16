# 040 — FST rewrite/metathesis compilation was blind to cross-table shared representations

## Kind
Behavioural (Rust-only: `pg_foma`'s FST proposer has no C# counterpart, but a candidate it fails to
propose never reaches `pg_parse::Morpher::confirm`, so the FST-optimized engine's delivered parses
fell short of the oracle it is supposed to only propose-for-then-confirm).

## Status
Fixed-in-rust.

## C# site
None — HermitCrab has no FST precompilation stage; it interprets every rule directly against
`FeatureStruct`-unifiable segments, so this class of bug cannot occur there at all.

## Rust site
`pg_foma::replace::SegAlphabet::token`/`render_tokens` and the new
`pg_foma::replace::RepresentationAliasMap` (`rust/crates/pg-foma/src/replace.rs`), consumed by
`compile_rewrite_rule_subset` (ordinary rewrite rules) and `compile_metathesis_swap_net` (metathesis
rules).

## What differs
`SegAlphabet::token` is `PUA_BASE + cd.0`, a pure function of a char-def's raw *per-table* index,
blind to which table produced it. When a rule compiled against one table (its own owning stratum's,
say `t1`) needed to match or render material that originated on a different table (`t0`) sharing the
same spelling at a *different* raw index, the rule's token for that spelling never matched the
material's own token — a table-blind FST proposer silently missed it. Fixed in two steps, for the two
call sites that mint tokens from char-defs:
- `compile_rewrite_rule_subset` (ordinary rewrite rules): `SegAlphabet` gained an optional aliasing
  field (`RepresentationAliasMap`) plus `with_table_id`, so a rule's own alphabet renders every
  feature-unifiable char-def across every table as the SAME token, not only its own table's.
  `encode_shape`/`encode_query` stay unaliased (a query must never become ambiguous).
- `compile_metathesis_swap_net`: a metathesis swap must echo the exact matched value at its
  transposed position, so aliasing could not reuse `render_tokens`' text-level union directly (that
  would let the swap net emit *any* aliased spelling regardless of which one actually matched);
  aliasing was applied to `slot_candidates`' candidate SET instead, leaving the per-branch
  cross-product construction — and its exact-echo guarantee — unchanged.

## Can it change a parse?
Yes, for the FST-optimized (`pangloss --engine=foma`) delivery path specifically: before the fix, a
table-blind rule net provably never fired on the other table's material (demonstrated directly by
building the pre-fix-equivalent net in each paired test), so `Morpher`-confirmable analyses the
oracle finds were never even proposed and so never delivered. Because HC-Rust's own architecture is
FST-propose-then-HC-confirm-only (never a full-HC fallback), a missed proposal is a real recall loss
in the engine actually shipped, not merely a missed optimization.

## Evidence
`conformance-staging/edge-cases/two-table-shared-representation-recall/` (rewrite-rule case) and
`conformance-staging/edge-cases/multi-table-metathesis-shared-representation/` (metathesis case), each
pinned directly by a paired Rust test — `rust/crates/pg-foma/tests/
two_table_shared_representation_recall.rs` and `rust/crates/pg-foma/tests/
multi_table_metathesis_shared_representation.rs` — that (a) builds the pre-fix-equivalent net and
shows the rule/swap silently fails to fire on the other table's material, (b) shows the current
(fixed) compile catches it, and (c) checks end-to-end containment against `pg_parse::Morpher` for
every word in the fixture. Fix commits: `0250853e` (rewrite-rule aliasing, task 4.4b) and `b946b401`
(the metathesis analog, task 4.2's residual). Both fixtures are additionally cross-checked by
`pg-parse/tests/conformance_fixtures_gate.rs`'s `all_discovered_fixtures_match_oracle`, which only
ever runs the oracle path (`pg_parse::Morpher`), never the FST proposer.

## Upstream
None, not applicable — pure Rust-side (`pg_foma`) proposer bug with no C# equivalent to report.

## Notes
`multi-table-metathesis-shared-representation`'s own STAGING.md documents two FURTHER, unrelated
bugs found while building that fixture, entirely inside the oracle (`pg_rules`, not `pg_foma`) —
see entries 041 and 042.
