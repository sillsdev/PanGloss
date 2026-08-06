# Bare-root compile-time discharge — why omitting the arc is provably safe

`rust/crates/pg-foma/tests/bare_root_compile_time_discharge.rs` pins a compile-time optimization:
omitting the bare-root (`"#"`-continuation) lexc arc for a root lexical entry that has exactly one
allomorph, and that allomorph is `isBound="true"`.

## Why this is provably safe, not a heuristic

This crate's root validity check treats a word as invalid whenever a bound root allomorph is the
word's only allomorph (`FailureReason::BoundRoot` — mirroring the C# `RootAllomorph.
CheckAllomorphConstraints` constraint that a bound allomorph cannot be the word's only allomorph;
pinned separately by `pg_rules`'s `bound_root_alone_is_rejected`). A bare-root candidate (the
`"#"`-continuation arc this test inspects) is by construction a word consisting of exactly that one
root morph and nothing else, so its allomorph-distinct-count is trivially `1` for every such
candidate. The validity gate therefore reduces, on this arc only, to `def.is_bound` alone — a fact
readable straight off `RootAllomorphDef::is_bound` with no live `Morpher` needed, whenever the
owning entry has exactly one allomorph (so there is no cross-allomorph free-fluctuation or
disjunctive-candidate reasoning to get wrong).

## What the test proves

1. The bound root's own bare (`"#"`) lexc line is absent from the emitted `Root` lexicon.
2. A free root's bare line is still present — the omission is per-entry, not a blanket bug.
3. Recall is unchanged: the bound root's suffixed word still confirms exactly like the oracle; the
   bare bound word confirms zero analyses under both the oracle and the FST propose-confirm
   pipeline (the arc removed was always dead weight, never a live analysis); the free root's bare
   word still confirms exactly one analysis, proving ordinary bare-root recall is untouched.

Assertion 1 is the one that fails with the fix reverted (unfixed code emits the bound root's bare
line unconditionally) and passes with it applied.
